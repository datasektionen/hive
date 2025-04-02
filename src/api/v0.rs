use rocket::request::FromParam;
#[cfg(feature = "api-docs")]
use rocket::{
    http::ContentType,
    response::{content::RawHtml, Redirect},
    routes,
};

use crate::routing::RouteTree;

mod token;
mod user;

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![
        token::routes(),
        user::routes(),
        #[cfg(feature = "api-docs")]
        routes![spec, docs, root].into(),
    ])
}

#[cfg(feature = "api-docs")]
#[rocket::get("/openapi.yaml")]
pub async fn spec() -> (ContentType, &'static str) {
    let r#type = ContentType::new("text", "yaml").with_params(("charset", "utf-8"));

    (r#type, include_str!("v0/openapi.yaml"))
}

#[cfg(feature = "api-docs")]
#[rocket::get("/docs")]
pub async fn docs() -> RawHtml<&'static str> {
    RawHtml(include_str!("docs.html"))
}

#[cfg(feature = "api-docs")]
#[rocket::get("/")]
pub async fn root() -> Redirect {
    Redirect::permanent("/api/v0/docs")
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
