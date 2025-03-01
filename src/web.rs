use rocket::{response::Redirect, uri};

use crate::routing::RouteTree;

mod groups;

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![groups::routes(), rocket::routes![favicon].into()])
}

#[rocket::get("/favicon.ico")]
async fn favicon() -> Redirect {
    // browsers expect favicon at root; redirect to real path
    Redirect::permanent(uri!("/static/icons/favicon.ico"))
}
