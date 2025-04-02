pub use catchers::catchers;

use crate::perms::HivePermission;

mod catchers;
pub mod v0;
pub mod v1;

pub struct ApiVersionInfo<'a> {
    pub n: u8,
    pub annotation: Option<(&'a str, &'a str)>, // en, sv
    pub deprecated: bool,
    pub recommended: bool, // e.g., not in beta
}

pub const API_VERSIONS: &[ApiVersionInfo<'static>] = &[
    ApiVersionInfo {
        n: 0,
        annotation: Some(("legacy", "äldre")),
        deprecated: true,
        recommended: false,
    },
    ApiVersionInfo {
        n: 1,
        annotation: Some(("preferred", "föredraget")),
        deprecated: false,
        recommended: true,
    },
];

#[derive(Clone)]
pub enum HiveApiPermission {
    CheckPermissions,
    ListTagged,
}

impl From<HiveApiPermission> for HivePermission {
    fn from(perm: HiveApiPermission) -> Self {
        match perm {
            HiveApiPermission::CheckPermissions => HivePermission::ApiCheckPermissions,
            HiveApiPermission::ListTagged => HivePermission::ApiListTagged,
        }
    }
}

#[cfg(not(feature = "api-docs"))]
macro_rules! with_api_docs {
    ($key:literal, $routes:expr) => {
        $routes
    };
}

#[cfg(feature = "api-docs")]
macro_rules! with_api_docs {
    ($key:literal, $routes:expr) => {{
        use rocket::{
            http::ContentType,
            response::{content::RawHtml, Redirect},
            routes,
        };

        #[rocket::get("/openapi.yaml")]
        pub async fn spec() -> (ContentType, &'static str) {
            let r#type = ContentType::new("text", "yaml").with_params(("charset", "utf-8"));

            (r#type, include_str!(concat!($key, "/openapi.yaml")))
        }

        #[rocket::get("/docs")]
        pub async fn docs() -> RawHtml<&'static str> {
            RawHtml(include_str!("docs.html"))
        }

        #[rocket::get("/")]
        pub async fn root() -> Redirect {
            Redirect::permanent(concat!("/api/", $key, "/docs"))
        }

        RouteTree::Branch(vec![$routes, routes![spec, docs, root].into()])
    }};
}

// required to allow the `allow()` below
#[allow(clippy::useless_attribute)]
// required for usage in this module's children
#[allow(clippy::needless_pub_self)]
pub(self) use with_api_docs;
