use chrono::{DateTime, Local};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(FromRow)]
pub struct Group {
    pub id: String,
    pub domain: String,
    pub name_sv: String,
    pub name_en: String,
    pub description_sv: String,
    pub description_en: String,
}

#[derive(FromRow)]
pub struct System {
    pub id: String,
    pub description: String,
}

#[derive(FromRow)]
pub struct ApiToken {
    pub id: Uuid,
    pub system_id: String,
    pub description: String,
    pub expires_at: Option<DateTime<Local>>,
    pub last_used_at: Option<DateTime<Local>>,
}

#[derive(FromRow)]
pub struct Permission {
    pub system_id: String,
    pub perm_id: String,
    pub has_scope: bool,
    pub description: String,
}

#[derive(FromRow)]
pub struct BasePermissionAssignment {
    pub system_id: String,
    pub perm_id: String,
    pub scope: Option<String>,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "action_kind", rename_all = "snake_case")]
pub enum ActionKind {
    Create,
    Update,
    Delete,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "target_kind", rename_all = "snake_case")]
pub enum TargetKind {
    Group,
    Membership,
    System,
    ApiToken,
    Tag,
    TagAssignment,
    Permission,
    PermissionAssignment,
}
