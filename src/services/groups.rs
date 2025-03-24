use std::ops::Add;

use serde::{Deserialize, Serialize};

use crate::{
    errors::{AppError, AppResult},
    models::GroupRef,
};

pub mod details;
pub mod list;
pub mod management;
pub mod members;
pub mod permissions;

pub enum GroupMembershipKind {
    Indirect,
    Direct,
}

pub enum RoleInGroup {
    Member,
    Manager,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityInGroup {
    None,
    View,
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

impl Add<&Option<RoleInGroup>> for AuthorityInGroup {
    type Output = Self;

    fn add(self, role: &Option<RoleInGroup>) -> Self::Output {
        if let Some(RoleInGroup::Manager) = role {
            self.max(AuthorityInGroup::ManageMembers)
        } else if role.is_some() {
            self.max(AuthorityInGroup::View)
        } else {
            self
        }
    }
}

pub struct GroupRelevance {
    pub role: Option<RoleInGroup>,
    pub authority: AuthorityInGroup,
    pub paths: Vec<Vec<GroupRef>>, // empty => not indirect member (but not <=!)
    pub is_direct_member: bool,    // false doesn't mean indirect! might be none
}

impl GroupRelevance {
    pub fn new(
        role: Option<RoleInGroup>,
        authority: AuthorityInGroup,
        mut paths: Vec<Vec<GroupRef>>,
    ) -> Option<Self> {
        if role.is_none() && authority == AuthorityInGroup::None {
            None
        } else {
            let mut is_direct_member = false;
            paths.retain(|path| {
                if path.is_empty() {
                    is_direct_member = true;

                    false
                } else {
                    true
                }
            });

            Some(Self {
                role,
                authority,
                paths,
                is_direct_member,
            })
        }
    }
}
