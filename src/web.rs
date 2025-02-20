use crate::{errors::AppResult, routing::RouteTree};

pub fn tree() -> RouteTree {
    rocket::routes![hello].into()
}

#[rocket::get("/hello")]
async fn hello() -> AppResult<&'static str> {
    Ok("hello")
}
