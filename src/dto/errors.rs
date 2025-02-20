use serde::Serialize;

use crate::errors::AppError;

#[derive(Serialize)]
#[serde(tag = "key", content = "context")]
enum InnerAppErrorDto {
    #[serde(rename = "db")]
    DbError,
}

impl From<AppError> for InnerAppErrorDto {
    fn from(err: AppError) -> Self {
        match err {
            AppError::DbError(..) => Self::DbError,
        }
    }
}

#[derive(Serialize)]
pub struct AppErrorDto {
    error: bool,
    info: InnerAppErrorDto,
}

impl From<AppError> for AppErrorDto {
    fn from(err: AppError) -> Self {
        Self {
            error: true,
            info: err.into(),
        }
    }
}
