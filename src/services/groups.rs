use serde::Serialize;

use crate::errors::{AppError, AppResult};

pub mod details;
pub mod list;
pub mod management;

pub enum GroupMembershipKind {
    Indirect,
    Direct,
}

pub enum RoleInGroup {
    Member,
    Manager,
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityInGroup {
    None,
    ManageMembers,
    FullyAuthorized,
}

impl AuthorityInGroup {
    pub fn require(&self, min: Self) -> AppResult<()> {
        if *self >= min {
            Ok(())
        } else {
            Err(AppError::InsufficientAuthorityInGroup(min))
        }
    }
}

pub struct GroupRelevance {
    pub role: Option<RoleInGroup>,
    pub authority: AuthorityInGroup,
}

impl GroupRelevance {
    pub fn new(role: Option<RoleInGroup>, authority: AuthorityInGroup) -> Option<Self> {
        if role.is_none() && authority == AuthorityInGroup::None {
            None
        } else {
            Some(Self { role, authority })
        }
    }
}
