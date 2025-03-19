use serde::{Deserialize, Serialize};

use crate::{errors::AppError, guards::lang::Language, services::groups::AuthorityInGroup};

#[derive(Serialize, Deserialize)]
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
            AppError::ErrorDecodeFailure => Self::PipelineError,
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

// this should probably be in locales/ with all other translations, but
// rust-i18n doesn't support passing arbitrary enum variant struct fields
// that well, and this way we get exhaustiveness-check for free
// (adding a new error type and forgetting to translate throws an error
// at compile-time, which would not happen via rust-i18n)
impl InnerAppErrorDto {
    fn title(&self, lang: &Language) -> &'static str {
        match (self, lang) {
            (Self::DbError, Language::English) => "Database Error",
            (Self::DbError, Language::Swedish) => "Databasfel",
            (Self::PipelineError, Language::English) => "Pipeline Error",
            (Self::PipelineError, Language::Swedish) => "Rörledningsfel",
            (Self::SelfPreservation, Language::English) => "Self-Preservation Fault",
            (Self::SelfPreservation, Language::Swedish) => "Självbevarelsedriftsfel",
            (Self::NotAllowed, Language::English) => "Not Allowed",
            (Self::NotAllowed, Language::Swedish) => "Inte tillåtet",
            (Self::InsufficientAuthorityInGroup { .. }, Language::English) => {
                "Insufficient Authority in Group"
            }
            (Self::InsufficientAuthorityInGroup { .. }, Language::Swedish) => {
                "Otillräcklig auktoritet i gruppen"
            }
            (Self::NoSuchSystem { .. }, Language::English) => "Unknown System",
            (Self::NoSuchSystem { .. }, Language::Swedish) => "Okänt system",
            (Self::NoSuchGroup { .. }, Language::English) => "Unknown Group",
            (Self::NoSuchGroup { .. }, Language::Swedish) => "Okänt grupp",
        }
    }

    fn description(&self, lang: &Language) -> String {
        match (self, lang) {
            (Self::DbError, Language::English) => "An error occurred when querying the database. \
                                                   Please try again later, or contact an \
                                                   administrator if the issue persists."
                .to_owned(),
            (Self::DbError, Language::Swedish) => "Ett fel uppstod vid förfrågan till databasen. \
                                                   Försök igen senare eller kontakta en \
                                                   administratör om problemet kvarstår."
                .to_owned(),
            (Self::PipelineError, Language::English) => {
                "An error occurred while processing your request. Please try again later, or \
                 contact an administrator if the issue persists."
                    .to_owned()
            }
            (Self::PipelineError, Language::Swedish) => {
                "Ett fel uppstod vid hantering av din begäran. Försök igen senare eller kontakta \
                 en administratör om problemet kvarstår."
                    .to_owned()
            }
            (Self::SelfPreservation, Language::English) => {
                "Your action was automatically disallowed because it compromises the system's \
                 integrity. This incident will be reported."
                    .to_owned()
            }
            (Self::SelfPreservation, Language::Swedish) => {
                "Din åtgärd avvisades automatiskt eftersom den äventyrar systemets integritet. \
                 Denna händelse kommer att rapporteras."
                    .to_owned()
            }
            (Self::NotAllowed, Language::English) => {
                "You lack the necessary permissions to perform this action.".to_owned()
            }
            (Self::NotAllowed, Language::Swedish) => {
                "Du har inte de nödvändiga behörigheterna för att utföra denna åtgärd.".to_owned()
            }
            (Self::InsufficientAuthorityInGroup { min }, Language::English) => format!(
                "You lack the necessary authority in the relevant group to perform this action. \
                 {} is required for access to be granted.",
                match min {
                    AuthorityInGroup::FullyAuthorized => "Full authority",
                    AuthorityInGroup::ManageMembers => "Member management authority",
                    AuthorityInGroup::None => "Nothing", // in theory, shouldn't happen
                }
            ),
            (Self::InsufficientAuthorityInGroup { min }, Language::Swedish) => format!(
                "Du saknar den nödvändiga befogenheten i den berörda gruppen för att utföra denna \
                 åtgärd. {} krävs för att få åtkomst.",
                match min {
                    AuthorityInGroup::FullyAuthorized => "Fullständig befogenhet",
                    AuthorityInGroup::ManageMembers => "Befogenhet att hantera medlemmar",
                    AuthorityInGroup::None => "Ingenting", // in theory, shouldn't happen
                }
            ),
            (Self::NoSuchSystem { id }, Language::English) => {
                format!("Could not find any system with ID {id}.")
            }
            (Self::NoSuchSystem { id }, Language::Swedish) => {
                format!("Kunde inte hitta något system med ID {id}.")
            }
            (Self::NoSuchGroup { id, domain }, Language::English) => {
                format!("Could not find any group with ID {id}@{domain}.")
            }
            (Self::NoSuchGroup { id, domain }, Language::Swedish) => {
                format!("Kunde inte hitta någon grupp med ID {id}@{domain}.")
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
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

impl AppErrorDto {
    pub fn title(&self, lang: &Language) -> &'static str {
        self.info.title(lang)
    }

    pub fn description(&self, lang: &Language) -> String {
        self.info.description(lang)
    }
}
