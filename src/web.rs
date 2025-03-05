use rocket::{
    response::{content::RawHtml, Redirect},
    uri,
};

use crate::routing::RouteTree;

mod groups;
mod systems;

type RenderedTemplate = RawHtml<String>;

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![
        groups::routes(),
        systems::routes(),
        rocket::routes![favicon].into(),
    ])
}

#[rocket::get("/favicon.ico")]
async fn favicon() -> Redirect {
    // browsers expect favicon at root; redirect to real path
    Redirect::permanent(uri!("/static/icons/favicon.ico"))
}
