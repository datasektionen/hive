use rocket::request::FromParam;

use super::with_api_docs;
use crate::routing::RouteTree;

mod token;
mod user;

pub fn tree() -> RouteTree {
    with_api_docs!(
        "v0",
        RouteTree::Branch(vec![token::routes(), user::routes()])
    )
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
