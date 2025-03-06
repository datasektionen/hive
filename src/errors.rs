use log::*;
use rocket::{
    http::Status,
    request::Outcome,
    response::{self, Responder},
    serde::json::Json,
    Request, Response,
};

use crate::{dto::errors::AppErrorDto, perms::HivePermission};

pub type AppResult<T> = Result<T, AppError>;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("database error: {0}")]
    DbError(#[from] sqlx::Error),
    #[error("template render error: {0}")]
    RenderError(#[from] rinja::Error),

    #[error("user lacks permissions to perform action (minimum needed: {0})")]
    NotAllowed(HivePermission),
    #[error("action disallowed because it compromises system integrity")]
    SelfPreservation,

    #[error("could not find system with ID `{0}`")]
    NoSuchSystem(String),
}

impl AppError {
    fn status(&self) -> Status {
        match self {
            AppError::DbError(..) => Status::InternalServerError,
            AppError::RenderError(..) => Status::InternalServerError,
            AppError::NotAllowed(..) => Status::Forbidden,
            AppError::SelfPreservation => Status::UnavailableForLegalReasons,
            AppError::NoSuchSystem(..) => Status::NotFound,
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
