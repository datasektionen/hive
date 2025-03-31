use serde::Serialize;

use crate::{models::BasePermissionAssignment, routing::RouteTree};

// API version 1 is the first primary edition of HTTP REST endpoints exposed by
// Hive (except for version 0, which should not be used by new code).
// All operations are relative to the invoker, as determined by the system
// associated with the API key passed via the HTTP `Authorization` header (see
// below). This means that anywhere a system ID is not passed, the current
// "relevant" one is used.
// The following endpoints are available (with the possibility of more being
// added in the future):

// GET /api/v1/user/<username>/permissions
//      Returns a ({ id: string, scope: string | null })[] with the user's
//      recognized permissions for the relevant system.
//
// GET /api/v1/user/<username>/permission/<perm_id>
//      Returns a boolean corresponding to whether the user is recognized to
//      have the given permission (for the relevant system). If the specified
//      permission is scoped, this endpoint always returns false, unless the
//      user is authorized for the wildcard scope (*).
//
// GET /api/v1/user/<username>/permission/<perm_id>/scopes
//      Returns a string[] with the user's recognized scopes for the given
//      permission (in the relevant system). If the specified permission is not
//      scoped, this endpoint always returns an empty array.
//
// GET /api/v1/user/<username>/permission/<perm_id>/scope/<scope>
//      Returns a boolean corresponding to whether the user is recognized to
//      have the given permission with the specified scope (or the * wildcard)
//      for the relevant system. If the specified permission is not scoped, this
//      endpoint always returns false.
//
// GET /api/v1/token/<api_token_secret>/permissions
//      Returns a ({ id: string, scope: string | null })[] with the API token's
//      recognized permissions for the relevant system.
//
// GET /api/v1/token/<api_token_secret>/permission/<perm_id>
//      Returns a boolean corresponding to whether the API token is recognized
//      to have the given permission (for the relevant system). If the specified
//      permission is scoped, this endpoint always returns false, unless the
//      API token is authorized for the wildcard scope (*).
//
// GET /api/v1/token/<api_token_secret>/permission/<perm_id>/scopes
//      Returns a string[] with the API token's recognized scopes for the given
//      permission (in the relevant system). If the specified permission is not
//      scoped, this endpoint always returns an empty array.
//
// GET /api/v1/token/<api_token_secret>/permission/<perm_id>/scope/<scope>
//      Returns a boolean corresponding to whether the API token is recognized
//      to have the given permission with the specified scope (or the *
//      wildcard) for the relevant system. If the specified permission is not
//      scoped, this endpoint always returns false.

// All these endpoints require HTTP authentication, which means that consumers
// must include an `Authorization` header in all requests following the format
// `Bearer <secret>`, where `<secret>` is the secret for a registered API token
// with appropriate `$hive:api-*` permissions. Endpoint results will implicitly
// be customized for the system with which the API token is associated.

mod token;
mod user;

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![token::routes(), user::routes()])
}

#[derive(Serialize)]
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
