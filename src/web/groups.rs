use askama_rocket::Template;
use rocket::State;
use sqlx::PgPool;

use crate::{errors::AppResult, models::Group, routing::RouteTree};

pub fn routes() -> RouteTree {
    rocket::routes![list_groups].into()
}

#[derive(Template)]
#[template(path = "groups/list.html.j2")]
struct ListGroupsView {
    groups: Vec<Group>,
}

#[rocket::get("/groups")]
async fn list_groups(db: &State<PgPool>) -> AppResult<ListGroupsView> {
    let groups = sqlx::query_as("SELECT * FROM groups")
        .fetch_all(db.inner())
        .await?;

    Ok(ListGroupsView { groups })
}
