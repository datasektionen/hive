use std::{fmt, hash};

use chrono::{DateTime, Local, NaiveDate};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    errors::AppResult,
    guards::{lang::Language, perms::PermsEvaluator},
    perms::{HivePermission, SystemsScope},
};

// these are only needed in other sqlx::Type composite type records
#[derive(sqlx::Type, PartialEq, Clone)]
#[sqlx(type_name = "slug")]
pub struct Slug(String);

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(sqlx::Type, PartialEq, Clone)]
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

#[derive(sqlx::Type, PartialEq, Clone)]
#[sqlx(type_name = "group_ref")]
pub struct GroupRef {
    pub group_id: Slug,
    pub group_domain: Domain,
}

// for when loading the whole Group isn't needed
// (e.g., just in an autocomplete listing with name and id@domain)
#[derive(FromRow, Clone)]
pub struct SimpleGroup {
    pub id: String,
    pub domain: String,
    pub name_sv: String,
    pub name_en: String,
}

impl SimpleGroup {
    pub fn key(&self) -> String {
        format!("{}@{}", self.id, self.domain)
    }

    pub fn localized_name(&self, lang: &Language) -> &str {
        match lang {
            Language::Swedish => &self.name_sv,
            Language::English => &self.name_en,
        }
    }
}

impl PartialEq for SimpleGroup {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.domain == other.domain
    }
}

impl Eq for SimpleGroup {}

impl hash::Hash for SimpleGroup {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.domain.hash(state);
    }
}

pub trait GroupModel: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin {}

impl GroupModel for Group {}
impl GroupModel for SimpleGroup {}

#[derive(FromRow)]
pub struct GroupMember {
    #[sqlx(default)]
    pub id: Option<Uuid>, // only exists for direct memberships
    pub username: String,
    pub from: NaiveDate,
    pub until: NaiveDate,
    pub manager: bool,
    #[sqlx(default)]
    pub display_name: Option<String>, // None if not loaded yet
}

impl GroupMember {
    pub fn is_direct_member(&self) -> bool {
        self.id.is_some()
    }
}

#[derive(FromRow)]
pub struct Subgroup {
    pub manager: bool,
    #[sqlx(flatten)]
    pub group: SimpleGroup,
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
    #[sqlx(default)]
    #[sqlx(try_from = "i64")]
    pub n_perms: usize, // number of assigned permissions
}

#[derive(FromRow)]
pub struct Permission {
    pub system_id: String,
    pub perm_id: String,
    pub has_scope: bool,
    pub description: String,
}

impl Permission {
    pub fn key(&self) -> String {
        format!("${}:{}", self.system_id, self.perm_id)
    }
}

#[derive(FromRow)]
pub struct PermissionAssignment {
    pub id: Uuid,
    pub system_id: String,
    pub perm_id: String,
    pub scope: Option<String>,
    pub description: String,
    #[sqlx(default)]
    pub can_manage: Option<bool>, // whether current user can e.g. unassign
}

impl PermissionAssignment {
    pub fn key(&self) -> String {
        format!("${}:{}", self.system_id, self.perm_id)
    }

    pub fn scoped_key_escaped(&self) -> String {
        if let Some(scope) = &self.scope {
            format!(
                "${}:{}:{}",
                self.system_id,
                self.perm_id,
                rinja::filters::escape(scope, rinja::filters::Html).expect("infallible")
            )
        } else {
            format!("${}:{}", self.system_id, self.perm_id)
        }
    }
}

#[derive(FromRow)]
pub struct BasePermissionAssignment {
    pub system_id: String,
    pub perm_id: String,
    pub scope: Option<String>,
}

#[derive(FromRow)]
pub struct AffiliatedPermissionAssignment {
    pub id: Uuid,
    pub system_id: String,
    pub perm_id: String,
    pub scope: Option<String>,
    pub group_id: Option<String>,
    pub group_domain: Option<String>,
    pub api_token_id: Option<Uuid>,
    #[sqlx(default)]
    pub api_token_system_id: Option<String>,
    #[sqlx(default)]
    pub label: Option<String>, // group name or token description
    #[sqlx(default)]
    pub can_manage: Option<bool>, // whether current user can e.g. unassign
}

impl AffiliatedPermissionAssignment {
    pub fn key(&self) -> String {
        format!("${}:{}", self.system_id, self.perm_id)
    }

    pub fn group_key(&self) -> Option<String> {
        if let Some(group_id) = &self.group_id {
            if let Some(group_domain) = &self.group_domain {
                return Some(format!("{}@{}", group_id, group_domain));
            }
        }

        None
    }
}

#[derive(FromRow)]
pub struct Tag {
    pub system_id: String,
    pub tag_id: String,
    pub supports_groups: bool,
    pub supports_users: bool,
    pub has_content: bool,
    pub description: String,
    #[sqlx(default)]
    pub can_view: Option<bool>, // whether current user can open tag details
}

impl Tag {
    pub fn key(&self) -> String {
        format!("#{}:{}", self.system_id, self.tag_id)
    }

    pub async fn set_can_view(&mut self, perms: &PermsEvaluator) -> AppResult<()> {
        let can_view = perms
            .satisfies_any_of(&[
                HivePermission::AssignTags(SystemsScope::Id(self.system_id.clone())),
                HivePermission::ManageTags(SystemsScope::Id(self.system_id.clone())),
            ])
            .await?;

        self.can_view = Some(can_view);

        Ok(())
    }
}

#[derive(FromRow)]
pub struct TagMorphology {
    pub has_content: bool,
    pub supports_groups: bool,
    pub supports_users: bool,
}

#[derive(FromRow)]
pub struct TagAssignment {
    pub id: Uuid,
    pub system_id: String,
    pub tag_id: String,
    pub content: Option<String>,
    pub description: String,
    #[sqlx(default)]
    pub can_manage: Option<bool>, // whether current user can e.g. unassign
}

impl TagAssignment {
    pub fn key(&self) -> String {
        format!("#{}:{}", self.system_id, self.tag_id)
    }

    pub fn contentful_key_escaped(&self) -> String {
        if let Some(content) = &self.content {
            format!(
                "#{}:{}:{}",
                self.system_id,
                self.tag_id,
                rinja::filters::escape(content, rinja::filters::Html).expect("infallible")
            )
        } else {
            format!("#{}:{}", self.system_id, self.tag_id)
        }
    }
}

#[derive(FromRow)]
pub struct AffiliatedTagAssignment {
    pub id: Option<Uuid>, // None if not a direct assignment
    pub system_id: String,
    pub tag_id: String,
    pub content: Option<String>,
    pub group_id: Option<String>,
    pub group_domain: Option<String>,
    pub username: Option<String>,
    #[sqlx(default)]
    pub label: Option<String>, // group name or user display name
    #[sqlx(default)]
    pub can_manage: Option<bool>, // whether current user can e.g. unassign
}

impl AffiliatedTagAssignment {
    pub fn key(&self) -> String {
        format!("#{}:{}", self.system_id, self.tag_id)
    }

    pub fn group_key(&self) -> Option<String> {
        if let Some(group_id) = &self.group_id {
            if let Some(group_domain) = &self.group_domain {
                return Some(format!("{}@{}", group_id, group_domain));
            }
        }

        None
    }
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "action_kind", rename_all = "snake_case")]
pub enum ActionKind {
    Create,
    Update,
    Delete,
    Impersonate,
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
    User,
}

#[derive(FromRow)]
pub struct IntegrationTaskRun {
    pub run_id: Uuid,
    pub task_id: String,
    pub start_stamp: DateTime<Local>,
    pub end_stamp: Option<DateTime<Local>>,
    pub succeeded: Option<bool>,
}

#[derive(FromRow)]
pub struct IntegrationTaskLogEntry {
    pub kind: IntegrationTaskLogEntryKind,
    pub stamp: DateTime<Local>,
    pub message: String,
}

#[derive(sqlx::Type, Clone, Copy)]
#[sqlx(
    type_name = "integration_task_log_entry_kind",
    rename_all = "snake_case"
)]
pub enum IntegrationTaskLogEntryKind {
    Error,
    Warning,
    Info,
}
