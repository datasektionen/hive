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
    #[serde(rename = "system.id.duplicate")]
    DuplicateSystemId { id: String },

    #[serde(rename = "api-token.description.ambiguous-in-system")]
    AmbiguousAPIToken { description: String },
    #[serde(rename = "permission.id.duplicate-in-system")]
    DuplicatePermissionId { id: String },

    #[serde(rename = "group.unknown")]
    NoSuchGroup { id: String, domain: String },
    #[serde(rename = "group.id.duplicate")]
    DuplicateGroupId { id: String, domain: String },
    #[serde(rename = "group.add.subgroup.invalid")]
    InvalidSubgroup { id: String, domain: String },
    #[serde(rename = "group.add.subgroup.duplicate")]
    DuplicateSubgroup { id: String, domain: String },
    #[serde(rename = "group.add.membership.redundant")]
    RedundantMembership { username: String },
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
            AppError::DuplicateSystemId(id) => Self::DuplicateSystemId { id },
            AppError::AmbiguousAPIToken(description) => Self::AmbiguousAPIToken { description },
            AppError::DuplicatePermissionId(id) => Self::DuplicatePermissionId { id },
            AppError::NoSuchGroup(id, domain) => Self::NoSuchGroup { id, domain },
            AppError::DuplicateGroupId(id, domain) => Self::DuplicateGroupId { id, domain },
            AppError::InvalidSubgroup(id, domain) => Self::InvalidSubgroup { id, domain },
            AppError::DuplicateSubgroup(id, domain) => Self::DuplicateSubgroup { id, domain },
            AppError::RedundantMembership(username) => Self::RedundantMembership { username },
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
            (Self::DuplicateSystemId { .. }, Language::English) => "Duplicate System ID",
            (Self::DuplicateSystemId { .. }, Language::Swedish) => "Duplicerat system-ID",
            (Self::AmbiguousAPIToken { .. }, Language::English) => {
                "Ambiguous API Token Description"
            }
            (Self::AmbiguousAPIToken { .. }, Language::Swedish) => "Tvetydig API-token beskrivning",
            (Self::DuplicatePermissionId { .. }, Language::English) => "Duplicate Permission ID",
            (Self::DuplicatePermissionId { .. }, Language::Swedish) => "Duplicerat behörighet-ID",
            (Self::NoSuchGroup { .. }, Language::English) => "Unknown Group",
            (Self::NoSuchGroup { .. }, Language::Swedish) => "Okänt grupp",
            (Self::DuplicateGroupId { .. }, Language::English) => "Duplicate Group ID",
            (Self::DuplicateGroupId { .. }, Language::Swedish) => "Duplicerat grupp-ID",
            (Self::InvalidSubgroup { .. }, Language::English) => "Invalid Subgroup",
            (Self::InvalidSubgroup { .. }, Language::Swedish) => "Ogiltig undergrupp",
            (Self::DuplicateSubgroup { .. }, Language::English) => "Duplicate Subgroup",
            (Self::DuplicateSubgroup { .. }, Language::Swedish) => "Duplicerat undergrupp",
            (Self::RedundantMembership { .. }, Language::English) => "Redundant Membership",
            (Self::RedundantMembership { .. }, Language::Swedish) => "Överflödigt medlemskap",
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
                    AuthorityInGroup::View => "Read authority",
                    AuthorityInGroup::None => "Nothing", // in theory, shouldn't happen
                }
            ),
            (Self::InsufficientAuthorityInGroup { min }, Language::Swedish) => format!(
                "Du saknar den nödvändiga befogenheten i den berörda gruppen för att utföra denna \
                 åtgärd. {} krävs för att få åtkomst.",
                match min {
                    AuthorityInGroup::FullyAuthorized => "Fullständig befogenhet",
                    AuthorityInGroup::ManageMembers => "Befogenhet att hantera medlemmar",
                    AuthorityInGroup::View => "Läsa befogenhet",
                    AuthorityInGroup::None => "Ingenting", // in theory, shouldn't happen
                }
            ),
            (Self::NoSuchSystem { id }, Language::English) => {
                format!("Could not find any system with ID \"{id}\".")
            }
            (Self::NoSuchSystem { id }, Language::Swedish) => {
                format!("Kunde inte hitta något system med ID \"{id}\".")
            }
            (Self::DuplicateSystemId { id }, Language::English) => {
                format!("ID \"{id}\" is already in use by another system.")
            }
            (Self::DuplicateSystemId { id }, Language::Swedish) => {
                format!("ID \"{id}\" används redan av ett annat system.")
            }
            (Self::AmbiguousAPIToken { description }, Language::English) => {
                format!(
                    "Description \"{description}\" is ambiguous because it is already in use by \
                     another API token for the same system."
                )
            }
            (Self::AmbiguousAPIToken { description }, Language::Swedish) => format!(
                "Beskrivning \"{description}\" är tvetydig eftersom den redan används av ett \
                 annat API-token för samma system."
            ),
            (Self::DuplicatePermissionId { id }, Language::English) => format!(
                "ID \"{id}\" is already in use by another permission associated with the same \
                 system."
            ),
            (Self::DuplicatePermissionId { id }, Language::Swedish) => format!(
                "ID \"{id}\" används redan av ett annan behörighet som är kopplad till samma \
                 system."
            ),
            (Self::NoSuchGroup { id, domain }, Language::English) => {
                format!("Could not find any group with ID \"{id}@{domain}\".")
            }
            (Self::NoSuchGroup { id, domain }, Language::Swedish) => {
                format!("Kunde inte hitta någon grupp med ID \"{id}@{domain}\".")
            }
            (Self::DuplicateGroupId { id, domain }, Language::English) => {
                format!("ID \"{id}\" is already in use by another group in domain \"{domain}\".")
            }
            (Self::DuplicateGroupId { id, domain }, Language::Swedish) => {
                format!("ID \"{id}\" används redan av en annan grupp i domänen \"{domain}\".")
            }
            (Self::InvalidSubgroup { id, domain }, Language::English) => {
                format!(
                    "The group with ID \"{id}@{domain}\" cannot be added as a subgroup to this \
                     group because it would lead to an infinite membership loop, since this group \
                     is already a (potentially indirect) subgroup of \"{id}@{domain}\"."
                )
            }
            (Self::InvalidSubgroup { id, domain }, Language::Swedish) => {
                format!(
                    "Gruppen med ID \"{id}@{domain}\" kan inte läggas till som en undergrupp till \
                     den här gruppen på grund av att den skulle leda till en oändlig medlemsloop, \
                     eftersom denna grupp redan är en (potentiellt indirekt) undergrupp av \
                     \"{id}@{domain}\"."
                )
            }
            (Self::DuplicateSubgroup { id, domain }, Language::English) => {
                format!("The group with ID \"{id}@{domain}\" is already a subgroup of this group.")
            }
            (Self::DuplicateSubgroup { id, domain }, Language::Swedish) => {
                format!("Gruppen med ID \"{id}@{domain}\" är redan en undergrupp till denna grupp.")
            }
            (Self::RedundantMembership { username }, Language::English) => {
                format!(
                    "User \"{username}\" is already a member of this group under the specified \
                     period with equivalent access rights."
                )
            }
            (Self::RedundantMembership { username }, Language::Swedish) => {
                format!(
                    "Användaren \"{username}\" är redan medlem i denna grupp under den angivna \
                     perioden med motsvarande åtkomsträttigheter."
                )
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
