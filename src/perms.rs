use std::{cmp::Ordering, fmt};

use chrono::Local;
use sqlx::PgPool;

use crate::{errors::AppResult, models::BasePermissionAssignment};

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub enum HivePermission {
    ViewLogs,
    ManageGroups(GroupsScope),
    ManageMembers(GroupsScope),
    ManageSystems,
    ManageSystem(SystemsScope),
    ManagePerms(SystemsScope),
}

impl HivePermission {
    pub const fn key(&self) -> &'static str {
        match self {
            Self::ViewLogs => "view-logs",
            Self::ManageGroups(..) => "manage-groups",
            Self::ManageMembers(..) => "manage-members",
            Self::ManageSystems => "manage-systems",
            Self::ManageSystem(..) => "manage-system",
            Self::ManagePerms(..) => "manage-perms",
        }
    }
}

impl fmt::Display for HivePermission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = self.key();

        match self {
            Self::ViewLogs | Self::ManageSystems => write!(f, "hive:{key}"),
            Self::ManageGroups(s) | Self::ManageMembers(s) => write!(f, "hive:{key}:{s}"),
            Self::ManageSystem(s) | Self::ManagePerms(s) => write!(f, "hive:{key}:{s}"),
        }
    }
}

impl PartialOrd for HivePermission {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        }

        match (self, other) {
            (Self::ManageGroups(a), Self::ManageGroups(b)) => a.partial_cmp(b),
            (Self::ManageMembers(a), Self::ManageMembers(b)) => a.partial_cmp(b),
            (Self::ManageSystem(a), Self::ManageSystem(b)) => a.partial_cmp(b),
            (Self::ManagePerms(a), Self::ManagePerms(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum InvalidHivePermissionError {
    Id,
    System,
    Scope,
}

impl TryFrom<BasePermissionAssignment> for HivePermission {
    type Error = InvalidHivePermissionError;

    fn try_from(perm: BasePermissionAssignment) -> Result<Self, Self::Error> {
        if perm.system_id != "hive" {
            return Err(InvalidHivePermissionError::System);
        }

        match (perm.perm_id.as_str(), perm.scope.as_deref()) {
            ("view-logs", None) => Ok(Self::ViewLogs),
            ("manage-groups", Some(scope)) => {
                let scope = GroupsScope::try_from(scope)?;

                Ok(Self::ManageGroups(scope))
            }
            ("manage-members", Some(scope)) => {
                let scope = GroupsScope::try_from(scope)?;

                Ok(Self::ManageMembers(scope))
            }
            ("manage-systems", None) => Ok(Self::ManageSystems),
            ("manage-system", Some(scope)) => {
                let scope = SystemsScope::try_from(scope)?;

                Ok(Self::ManageSystem(scope))
            }
            ("manage-perms", Some(scope)) => {
                let scope = SystemsScope::try_from(scope)?;

                Ok(Self::ManagePerms(scope))
            }
            _ => Err(InvalidHivePermissionError::Id),
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub enum GroupsScope {
    Wildcard,
    Tag(String),
    Domain(String),
}

impl TryFrom<&str> for GroupsScope {
    type Error = InvalidHivePermissionError;

    fn try_from(scope: &str) -> Result<Self, Self::Error> {
        if scope == "*" {
            Ok(Self::Wildcard)
        } else if let Some(tag) = scope.strip_prefix("#hive:") {
            Ok(Self::Tag(tag.to_owned()))
        } else if let Some(domain) = scope.strip_prefix("@") {
            Ok(Self::Domain(domain.to_owned()))
        } else {
            Err(InvalidHivePermissionError::Scope)
        }
    }
}

impl fmt::Display for GroupsScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wildcard => write!(f, "*"),
            Self::Tag(tag) => write!(f, "#hive:{tag}"),
            Self::Domain(domain) => write!(f, "@{domain}"),
        }
    }
}

impl PartialOrd for GroupsScope {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        }

        match (self, other) {
            (Self::Wildcard, _) => Some(Ordering::Greater),
            (_, Self::Wildcard) => Some(Ordering::Less),
            _ => None,
        }
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone)]
pub enum SystemsScope {
    Wildcard,
    Id(String),
}

impl TryFrom<&str> for SystemsScope {
    type Error = InvalidHivePermissionError;

    fn try_from(scope: &str) -> Result<Self, Self::Error> {
        if scope == "*" {
            Ok(Self::Wildcard)
        } else {
            Ok(Self::Id(scope.to_owned()))
        }
    }
}

impl fmt::Display for SystemsScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wildcard => write!(f, "*"),
            Self::Id(id) => write!(f, "{id}"),
        }
    }
}

impl PartialOrd for SystemsScope {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if self == other {
            return Some(Ordering::Equal);
        }

        match (self, other) {
            (Self::Wildcard, _) => Some(Ordering::Greater),
            (_, Self::Wildcard) => Some(Ordering::Less),
            _ => None,
        }
    }
}

pub async fn get_assignments(
    username: &str,
    system_id: &str,
    perm_id: &str,
    db: &PgPool,
) -> AppResult<Vec<BasePermissionAssignment>> {
    let today = Local::now().date_naive();

    let assignments = sqlx::query_as::<_, BasePermissionAssignment>(
        "
        SELECT *
        FROM permission_assignments pa
        JOIN all_groups_of($1, $2) gs
            ON pa.group_id = gs.id
            AND pa.group_domain = gs.domain
        WHERE system_id = $3
        AND perm_id = $4",
    )
    .bind(username)
    .bind(today)
    .bind(system_id)
    .bind(perm_id)
    .fetch_all(db)
    .await?;

    // can't use `fetch` instead of `fetch_all` (which would avoid deserializing
    // unless needed) because we want to cache *all* permission assignments;
    // this is fine under the assumption that there will be very few assignments
    // for the same (user, system, perm) triplet -- i.e., different scopes

    Ok(assignments)
}
