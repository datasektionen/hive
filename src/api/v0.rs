use rocket::request::FromParam;

use crate::routing::RouteTree;

// API version 0 is intended for usage by legacy systems, since it strives to be
// maximally backwards-compatible with some of the primary endpoints exposed by
// by the existing https://github.com/datasektionen/pls REST API. In particular,
// the following (query-only, not management) endpoints are implemented by Hive:

// GET /api/v0/user/:username
//      Returns an object { system_id: string[] } with the user's recognized
//      permissions for each system. String format is `perm_id:scope` for
//      scoped permissions and just `perm_id` otherwise.
//
// GET /api/v0/user/:username/:system_id
//      Returns a string[] with the user's recognized permissions for the given
//      system. String format is the same as above.
//
// GET /api/v0/user/:username/:system_id/:permission
//      Returns a boolean corresponding to whether the user is recognized to
//      have the given permission, which is provided in the same string format
//      as specified above. If the permission is scoped and a scope is not
//      provided, this endpoint always returns false, unless the user is
//      authorized for the wildcard scope (*).
//
// GET /api/v0/token/:api_token_secret/:system_id
//      Returns a string[] with the API token's recognized permissions for the
//      given system. String format is the same as above.
//
// GET /api/v0/token/:api_token_secret/:system_id/:permission
//      Returns a boolean corresponding to whether the API token is recognized
//      to have the given permission, which is provided in the same string
//      format as specified above. If the permission is scoped and a scope is not
//      provided, this endpoint always returns false, unless the user is
//      authorized for the wildcard scope (*).

// None of these endpoints require any form of authentication.

mod token;
mod user;

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![token::routes(), user::routes()])
}

struct PermKey<'r> {
    perm_id: &'r str,
    scope: Option<&'r str>,
}

impl<'r> FromParam<'r> for PermKey<'r> {
    type Error = ();

    fn from_param(value: &'r str) -> Result<Self, Self::Error> {
        if let Some((perm_id, scope)) = value.split_once(':') {
            if !scope.is_empty() {
                return Ok(Self {
                    perm_id,
                    scope: Some(scope),
                });
            }
        }

        Ok(Self {
            perm_id: value,
            scope: None,
        })
    }
}
