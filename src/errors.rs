use std::io::Cursor;

use log::*;
use rinja::Template;
use rocket::{
    fairing::{self, Fairing},
    http::{ContentType, Status},
    request::Outcome,
    response::{self, Responder},
    serde::json::Json,
    Request, Response,
};

use crate::{
    dto::errors::AppErrorDto,
    guards::{context::PageContext, headers::HxRequest},
    perms::HivePermission,
    services::groups::AuthorityInGroup,
};

pub type AppResult<T> = Result<T, AppError>;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("database error: {0}")]
    DbError(#[from] sqlx::Error),
    #[error("query building error: {0}")]
    QueryBuildError(#[source] sqlx::error::BoxDynError),
    #[error("template render error: {0}")]
    RenderError(#[from] rinja::Error),
    #[error("failed to decode error while generating error page from JSON")]
    ErrorDecodeFailure,

    #[error("user lacks permissions to perform action (minimum needed: {0})")]
    NotAllowed(HivePermission),
    #[error("user lacks necessary authority in group (minimum needed: {0:?}")]
    InsufficientAuthorityInGroup(AuthorityInGroup),
    #[error("action disallowed because it compromises system integrity")]
    SelfPreservation,

    #[error("could not find system with ID `{0}`")]
    NoSuchSystem(String),
    #[error("ID `{0}` is already in use by another system")]
    DuplicateSystemId(String),

    #[error("description `{0}` is already in use by another API token for this system")]
    AmbiguousAPIToken(String),

    #[error("could not find permission with key `${0}:{1}`")]
    NoSuchPermission(String, String),
    #[error("ID `{0}` is already in use by another permission for this system")]
    DuplicatePermissionId(String),
    #[error("permission `${0}:{1}:{scope}` is already assigned to this entity", scope = .2.as_deref().unwrap_or("/"))]
    DuplicatePermissionAssignment(String, String, Option<String>),
    #[error("permission with key `${0}:{1}` requires a scope to be specified on assignment")]
    MissingPermissionScope(String, String),
    #[error("permission with key `${0}:{1}` does not accept a scope on assignment")]
    ExtraneousPermissionScope(String, String),

    #[error("could not find group with key `{0}@{1}`")]
    NoSuchGroup(String, String),
    #[error("ID `{0}` is already in use by another group in domain `{1}`")]
    DuplicateGroupId(String, String),
    #[error("group `{0}@{1}` cannot be a subgroup of this system (loop detected)")]
    InvalidSubgroup(String, String),
    #[error("group with key `{0}@{1}` is already a subgroup of this system")]
    DuplicateSubgroup(String, String),
    #[error("user `{0}` is already a member of this group within the specified period")]
    RedundantMembership(String),
}

impl AppError {
    // normally any sqlx::Error is mapped to Self::DbError, but sometimes it's
    // necessary to use a more specific variant if a uniqueness constraint
    // violation is detected. this function keeps self iff the inner database
    // error is a unique violation, otherwise converts it into a DbError
    pub fn if_unique_violation(self, err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db_err) = err {
            if db_err.is_unique_violation() {
                return self;
            }
        }

        Self::from(err)
    }

    fn status(&self) -> Status {
        match self {
            AppError::DbError(..) => Status::InternalServerError,
            AppError::QueryBuildError(..) => Status::InternalServerError,
            AppError::RenderError(..) => Status::InternalServerError,
            AppError::ErrorDecodeFailure => Status::InternalServerError,
            AppError::NotAllowed(..) => Status::Forbidden,
            AppError::InsufficientAuthorityInGroup(..) => Status::Forbidden,
            AppError::SelfPreservation => Status::UnavailableForLegalReasons,
            AppError::NoSuchSystem(..) => Status::NotFound,
            AppError::DuplicateSystemId(..) => Status::Conflict,
            AppError::AmbiguousAPIToken(..) => Status::Conflict,
            AppError::NoSuchPermission(..) => Status::NotFound,
            AppError::DuplicatePermissionId(..) => Status::Conflict,
            AppError::DuplicatePermissionAssignment(..) => Status::Conflict,
            AppError::MissingPermissionScope(..) => Status::BadRequest,
            AppError::ExtraneousPermissionScope(..) => Status::BadRequest,
            AppError::NoSuchGroup(..) => Status::NotFound,
            AppError::DuplicateGroupId(..) => Status::Conflict,
            AppError::InvalidSubgroup(..) => Status::BadRequest,
            AppError::DuplicateSubgroup(..) => Status::Conflict,
            AppError::RedundantMembership(..) => Status::Conflict,
        }
    }
}

impl<'r> Responder<'r, 'static> for AppError {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let status = self.status();
        if status.code >= 500 {
            // debug prints enum variant name, display shows thiserror message
            error!("While handling [{req}], encountered {self:?}: {self}");
        } else {
            debug!("While handling [{req}], encountered {self:?}: {self}")
        }

        let base = Json(AppErrorDto::from(self)).respond_to(req)?;

        Ok(Response::build_from(base).status(status).finalize())
    }
}

impl<T> From<AppError> for Outcome<T, AppError> {
    fn from(err: AppError) -> Self {
        Outcome::Error((err.status(), err))
    }
}

#[derive(Template)]
#[template(path = "errors/full.html.j2")]
struct ErrorOccurredView {
    ctx: PageContext,
    title: String,
    description: String,
}

#[derive(Template)]
#[template(path = "errors/partial.html.j2")]
struct PartialErrorOccurredView {
    title: String,
    description: String,
}

// FIXME: this should become a typed catcher when Rocket implements the feature
// see https://github.com/rwf2/Rocket/issues/749#issuecomment-2024072120
pub struct ErrorPageGenerator;

#[rocket::async_trait]
impl Fairing for ErrorPageGenerator {
    fn info(&self) -> fairing::Info {
        fairing::Info {
            name: "Error Page Generator",
            kind: fairing::Kind::Response,
        }
    }

    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        // since we can't use await in Responder (respond_to is not async),
        // it's not possible to render an HTML webpage with PageContext
        // directly from AppError, so instead this fairing intercepts the
        // generated JSON response and, when relevant, converts it to a proper
        // page. this does mean, however, that we need to serialize to JSON
        // only to immediately deserialize it back, but the performance
        // penalty shouldn't be too heavy, especially since errors should be
        // a minority of all traffic, in general

        let status_class = res.status().class();
        if !status_class.is_client_error() && !status_class.is_server_error() {
            // nothing to do if there was no error
            return;
        }

        if let Some(route) = req.route() {
            if route.uri.base().starts_with("/api") {
                // nothing to do; error is already in JSON as intended
                return;
            }
        }

        if res.content_type().map(|t| t.is_html()).unwrap_or(false) {
            // this is not JSON! probably an error that has already been made
            // into HTML by a catcher
            return;
        }

        let mut error = AppErrorDto::from(AppError::ErrorDecodeFailure);

        if let Ok(body) = res.body_mut().to_string().await {
            if let Ok(dto) = serde_json::from_str(&body) {
                error = dto;
            }
        }

        let ctx = req
            .guard::<PageContext>()
            .await
            .expect("infallible page context guard");

        let title = error.title(&ctx.lang).to_owned();
        let description = error.description(&ctx.lang);

        res.set_header(ContentType::HTML);

        let partial = req.guard::<HxRequest>().await.succeeded();
        if partial.is_some() {
            // only oob swaps should take place
            res.set_raw_header("HX-Reswap", "none");
        }

        let html = render_error_page(title, description, res.status(), ctx, partial.is_some());
        res.set_sized_body(html.len(), Cursor::new(html));
    }
}

pub fn render_error_page<T: ToString, D: ToString>(
    title: T,
    description: D,
    status: Status,
    ctx: PageContext,
    partial: bool,
) -> String {
    let title = title.to_string();
    let description = description.to_string();

    if partial {
        let template = PartialErrorOccurredView { title, description };

        template.render().unwrap_or_else(|e| {
            error!("Failed to render partial error page: {e}");

            status.reason_lossy().to_owned()
        })
    } else {
        let template = ErrorOccurredView {
            ctx,
            title,
            description,
        };

        template.render().unwrap_or_else(|e| {
            error!("Failed to render full error page: {e}");

            status.reason_lossy().to_owned()
        })
    }
}
