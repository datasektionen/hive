use std::fmt;

use chrono::{DateTime, Local};
use sqlx::FromRow;
use uuid::Uuid;

use crate::guards::lang::Language;

// these are only needed in other sqlx::Type composite type records
#[derive(sqlx::Type)]
#[sqlx(type_name = "slug")]
pub struct Slug(String);

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "domain")]
pub struct Domain(String);

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(FromRow)]
pub struct Group {
    pub id: String,
    pub domain: String,
    pub name_sv: String,
    pub name_en: String,
    pub description_sv: String,
    pub description_en: String,
}

impl Group {
    pub fn key(&self) -> String {
        format!("{}@{}", self.id, self.domain)
    }

    pub fn localized_name(&self, lang: &Language) -> &str {
        match lang {
            Language::Swedish => &self.name_sv,
            Language::English => &self.name_en,
        }
    }

    pub fn localized_description(&self, lang: &Language) -> &str {
        match lang {
            Language::Swedish => &self.description_sv,
            Language::English => &self.description_en,
        }
    }
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "group_ref")]
pub struct GroupRef {
    pub group_id: Slug,
    pub group_domain: Domain,
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

impl Permission {
    pub fn full_id(&self) -> String {
        format!("${}:{}", self.system_id, self.perm_id)
    }
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
