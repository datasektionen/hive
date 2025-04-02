use serde::Serialize;

use crate::{models::BasePermissionAssignment, routing::RouteTree};

use super::with_api_docs;

mod tagged;
mod token;
mod user;

pub fn tree() -> RouteTree {
    with_api_docs!(
        "v1",
        RouteTree::Branch(vec![tagged::routes(), token::routes(), user::routes()])
    )
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
