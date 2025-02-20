use askama_rocket::Template;

use crate::{errors::AppResult, routing::RouteTree};

pub fn tree() -> RouteTree {
    rocket::routes![hello].into()
}

#[derive(Template)]
#[template(path = "hello.html.j2")]
struct HelloTemplate<'a> {
    name: &'a str,
}

#[rocket::get("/hello")]
async fn hello() -> AppResult<HelloTemplate<'static>> {
    Ok(HelloTemplate { name: "John" })
}
