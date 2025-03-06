use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    http::Header,
    response::{content::RawHtml, Redirect},
    uri, Responder, State,
};
use serde_json::json;
use sqlx::PgPool;

use super::{filters, RenderedTemplate};
use crate::{
    dto::systems::CreateSystemDto,
    errors::{AppError, AppResult},
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{ActionKind, System, TargetKind},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    sanitizers::SearchTerm,
};

pub fn routes() -> RouteTree {
    rocket::routes![list_systems, create_system, system_details].into()
}

#[derive(Template)]
#[template(path = "systems/list.html.j2")]
struct ListSystemsView<'q, 'f, 'v> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
    create_form: &'f form::Context<'v>,
    create_modal_open: bool,
}

// FIXME: separate Partial struct is only needed until the next Askama/Rinja
// release; after that use new attr `blocks` (feature-gated) to impl many
// methods for the same template struct
#[derive(Template)]
#[template(path = "systems/list.html.j2", block = "inner_systems_listing")]
struct PartialListSystemsView<'q> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
}

#[derive(Template)]
#[template(path = "systems/create.html.j2", block = "inner_create_form")]
struct PartialCreateSystemView<'f, 'v> {
    ctx: PageContext,
    create_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "systems/details.html.j2")]
struct SystemDetailsView {
    ctx: PageContext,
    system: System,
    fully_authorized: bool,
}

#[rocket::get("/systems?<q>")]
async fn list_systems(
    q: Option<&str>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: support partial listing; ManageSystem(something)
    // (use `let systems = if all { query all } else { query some }`)

    let mut query = sqlx::QueryBuilder::new("SELECT * FROM systems");

    if let Some(search) = q {
        // this will push the same bind twice even though both could be
        // references to the same $1 param... there doesn't seem to be a
        // way to avoid this, since push_bind adds $n to the query itself
        let term = SearchTerm::from(search).anywhere();
        query.push(" WHERE id ILIKE ");
        query.push_bind(term.clone());
        query.push(" OR description ILIKE ");
        query.push_bind(term);
    }

    let systems = query.build_query_as().fetch_all(db.inner()).await?;

    if partial.is_some() {
        let template = PartialListSystemsView { ctx, systems, q };

        Ok(RawHtml(template.render()?))
    } else {
        let template = ListSystemsView {
            ctx,
            systems,
            q,
            create_form: &form::Context::default(),
            create_modal_open: false,
        };

        Ok(RawHtml(template.render()?))
    }
}

#[derive(Responder)]
enum CreateSystemResponse {
    HtmxRedirect((), Header<'static>),
    HttpRedirect(Box<Redirect>), // large variant size difference
    ValidationError(RenderedTemplate),
}

#[rocket::post("/systems", data = "<form>")]
async fn create_system<'v>(
    form: Form<Contextual<'v, CreateSystemDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<CreateSystemResponse> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        let mut txn = db.begin().await?;

        let id: String = sqlx::query_scalar(
            "INSERT INTO systems (id, description) VALUES ($1, $2) RETURNING id",
        )
        .bind(dto.id)
        .bind(dto.description)
        .fetch_one(&mut *txn)
        .await?;

        sqlx::query(
            "INSERT INTO audit_logs (action_kind, target_kind, target_id, actor, details) VALUES \
             ($1, $2, $3, $4, $5)",
        )
        .bind(ActionKind::Create)
        .bind(TargetKind::System)
        .bind(&id)
        .bind(user.username)
        .bind(json!({"new": {"description": dto.description}}))
        .execute(&mut *txn)
        .await?;

        txn.commit().await?;

        let target = uri!(system_details(id));
        if partial.is_some() {
            let header = Header::new("HX-Redirect", target.to_string());
            Ok(CreateSystemResponse::HtmxRedirect((), header))
        } else {
            let redirect = Redirect::to(target);
            Ok(CreateSystemResponse::HttpRedirect(Box::new(redirect)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Create system form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialCreateSystemView {
                ctx,
                create_form: &form.context,
            };

            Ok(CreateSystemResponse::ValidationError(RawHtml(
                template.render()?,
            )))
        } else {
            let systems = sqlx::query_as("SELECT * FROM systems ORDER BY id")
                .fetch_all(db.inner())
                .await?;

            let template = ListSystemsView {
                ctx,
                systems,
                q: None,
                create_form: &form.context,
                create_modal_open: true,
            };

            Ok(CreateSystemResponse::ValidationError(RawHtml(
                template.render()?,
            )))
        }
    }
}

#[rocket::get("/system/<id>")]
pub async fn system_details(
    id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
) -> AppResult<RenderedTemplate> {
    let fully_authorized = perms.satisfies(HivePermission::ManageSystems).await?;

    if !fully_authorized {
        let scope = SystemsScope::Id(id.to_owned());
        perms.require(HivePermission::ManageSystem(scope)).await?;
    }

    let system = sqlx::query_as("SELECT * FROM systems WHERE id = $1")
        .bind(id)
        .fetch_optional(db.inner())
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;
    // ^ note: there is no enumeration vulnerability in returning 404 here
    // because we already checked that the user has perms to see all systems

    let template = SystemDetailsView {
        ctx,
        system,
        fully_authorized,
    };

    Ok(RawHtml(template.render()?))
}
