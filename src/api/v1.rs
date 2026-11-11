use chrono::NaiveDate;
use serde::Serialize;
use uuid::Uuid;

use super::with_api_docs;
use crate::{models::{AffiliatedTagAssignment, BasePermissionAssignment, GroupMember}, routing::RouteTree};

mod tagged;
mod token;
mod user;
mod webhook;
mod group;

pub fn tree() -> RouteTree {
    with_api_docs!(
        "v1",
        RouteTree::Branch(vec![
            tagged::routes(),
            token::routes(),
            user::routes(),
            webhook::routes(),
            group::routes()
        ])
    )
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct TaggedGroup {
    group_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_description: Option<String>,
    group_domain: String, // should be ordered first since it's shown separately
    group_id: String,
    tag_content: Option<String>,
}

impl From<AffiliatedTagAssignment> for TaggedGroup {
    fn from(assignment: AffiliatedTagAssignment) -> Self {
        Self {
            group_domain: assignment.group_domain.unwrap_or_default(),
            group_id: assignment.group_id.unwrap_or_default(),
            tag_content: assignment.content,
            group_name: assignment.label.unwrap_or_default(),
            group_description: assignment.description,
        }
    }
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaggedUser {
    username: String,
    tag_content: Option<String>,
}

impl From<AffiliatedTagAssignment> for TaggedUser {
    fn from(assignment: AffiliatedTagAssignment) -> Self {
        Self {
            username: assignment.username.unwrap_or_default(),
            tag_content: assignment.content,
        }
    }
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct SystemPermissionAssignment {
    pub id: String,
    pub scope: Option<String>,
}

impl From<BasePermissionAssignment> for SystemPermissionAssignment {
    fn from(assignment: BasePermissionAssignment) -> Self {
        Self {
            id: assignment.perm_id,
            scope: assignment.scope,
        }
    }
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct DetailedMember {
    id: Option<Uuid>,
    username: String,
    manager: bool,
    from: NaiveDate,
    until: NaiveDate,
}

impl From<GroupMember> for DetailedMember {
    fn from(value: GroupMember) -> Self {
        Self {
            id: value.id,
            username: value.username,
            manager: value.manager,
            from: value.from,
            until: value.until,
        }
    }
}

