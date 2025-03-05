use rinja::Template;
use rocket::{response::content::RawHtml, State};
use sqlx::PgPool;

use super::RenderedTemplate;
use crate::{errors::AppResult, guards::context::PageContext, models::Group, routing::RouteTree};

pub fn routes() -> RouteTree {
    rocket::routes![list_groups].into()
}

#[derive(Template)]
#[template(path = "groups/list.html.j2")]
struct ListGroupsView {
    ctx: PageContext,
    groups: Vec<Group>,
}

#[rocket::get("/groups")]
async fn list_groups(db: &State<PgPool>, ctx: PageContext) -> AppResult<RenderedTemplate> {
    let groups = sqlx::query_as("SELECT * FROM groups")
        .fetch_all(db.inner())
        .await?;

    let template = ListGroupsView { ctx, groups };

    Ok(RawHtml(template.render()?))
}
