use serde::Serialize;

use crate::{errors::AppError, services::groups::AuthorityInGroup};

#[derive(Serialize)]
#[serde(tag = "key", content = "context")]
enum InnerAppErrorDto {
    #[serde(rename = "db")]
    DbError,
    #[serde(rename = "pipeline")]
    PipelineError, // anything related to handling requests/responses (500)
    #[serde(rename = "self-preservation")]
    SelfPreservation,

    #[serde(rename = "forbidden")]
    NotAllowed,
    #[serde(rename = "group.forbidden")]
    InsufficientAuthorityInGroup { min: AuthorityInGroup },

    #[serde(rename = "system.unknown")]
    NoSuchSystem { id: String },
    #[serde(rename = "group.unknown")]
    NoSuchGroup { id: String, domain: String },
}

impl From<AppError> for InnerAppErrorDto {
    fn from(err: AppError) -> Self {
        match err {
            AppError::DbError(..) => Self::DbError,
            AppError::QueryBuildError(..) => Self::PipelineError,
            AppError::RenderError(..) => Self::PipelineError,
            AppError::NotAllowed(..) => Self::NotAllowed,
            AppError::InsufficientAuthorityInGroup(min) => {
                Self::InsufficientAuthorityInGroup { min }
            }
            AppError::SelfPreservation => Self::SelfPreservation,
            AppError::NoSuchSystem(id) => Self::NoSuchSystem { id },
            AppError::NoSuchGroup(id, domain) => Self::NoSuchGroup { id, domain },
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
