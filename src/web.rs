pub use catchers::catchers;
use rinja::Template;
use rocket::{
    http::{uri::Reference, Header},
    response::{content::RawHtml, Redirect},
    uri, Responder,
};

use crate::{
    api::{self, ApiVersionInfo},
    errors::AppResult,
    guards::{context::PageContext, lang::Language},
    routing::RouteTree,
};

mod api_tokens;
mod catchers;
mod groups;
mod permissions;
mod systems;
mod tags;

type RenderedTemplate = RawHtml<String>;

#[derive(Responder)]
enum GracefulRedirect {
    HtmxRedirect((), Header<'static>),
    HttpRedirect(Box<Redirect>), // boxed due to large variant size difference
}

impl GracefulRedirect {
    pub fn to<U>(target: U, partial: bool) -> Self
    where
        U: TryInto<Reference<'static>> + ToString,
    {
        if partial {
            let header = Header::new("HX-Redirect", target.to_string());
            Self::HtmxRedirect((), header)
        } else {
            Self::HttpRedirect(Box::new(Redirect::to(target)))
        }
    }
}

#[derive(Responder)]
enum Either<T, U> {
    Left(T),
    Right(U),
}

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![
        api_tokens::routes(),
        groups::routes(),
        permissions::routes(),
        systems::routes(),
        tags::routes(),
        rocket::routes![favicon, api_versions].into(),
    ])
}

#[rocket::get("/favicon.ico")]
fn favicon() -> Redirect {
    // browsers expect favicon at root; redirect to real path
    Redirect::permanent(uri!("/static/icons/favicon.ico"))
}

#[derive(Template)]
#[template(path = "api-versions.html.j2")]
struct ApiVersionsView<'v> {
    ctx: PageContext,
    versions: &'v [ApiVersionInfo<'v>],
    docs: bool,
}

#[rocket::get("/api")]
fn api_versions(ctx: PageContext) -> AppResult<RenderedTemplate> {
    let template = ApiVersionsView {
        ctx,
        versions: api::API_VERSIONS,
        docs: cfg!(feature = "api-docs"),
    };

    Ok(RawHtml(template.render()?))
}

mod filters {
    use chrono::{DateTime, Local, TimeZone};
    use regex::RegexBuilder;
    use rinja::filters::Safe;

    pub fn highlight<T: ToString>(s: Safe<T>, term: &str) -> rinja::Result<Safe<String>> {
        let s = s.0.to_string();

        let result = if term.is_empty() {
            s
        } else {
            let re = RegexBuilder::new(&regex::escape(term))
                .case_insensitive(true)
                .build()
                .unwrap();

            re.replace_all(&s, "<mark>$0</mark>").to_string()
        };

        Ok(Safe(result))
    }

    pub fn timestamp<Tz: TimeZone>(stamp: &DateTime<Tz>) -> rinja::Result<String> {
        Ok(format!(
            "{}",
            stamp.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S")
        ))
    }
}
