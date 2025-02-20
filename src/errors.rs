use log::*;
use rocket::{http::Status, response, response::Responder, serde::json::Json, Request, Response};

use crate::dto::errors::AppErrorDto;

pub type AppResult<T> = Result<T, AppError>;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("database error: {0}")]
    DbError(#[from] std::io::Error), // FIXME: proper inner type
}

impl AppError {
    fn status(&self) -> Status {
        match self {
            AppError::DbError(..) => Status::InternalServerError,
        }
    }
}

impl<'r> Responder<'r, 'static> for AppError {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'static> {
        let status = self.status();
        if status.code >= 500 {
            // debug prints enum variant name, display shows thiserror message
            error!("While handling [{req}], encountered {self:?}: {self}");
        }

        let base = Json(AppErrorDto::from(self)).respond_to(req)?;

        Ok(Response::build_from(base).status(status).finalize())
    }
}
