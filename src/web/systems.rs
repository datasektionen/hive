use askama_rocket::Template;
use rocket::State;
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, perms::PermsEvaluator},
    models::System,
    perms::HivePermission,
    routing::RouteTree,
};

pub fn routes() -> RouteTree {
    rocket::routes![list_systems].into()
}

#[derive(Template)]
#[template(path = "systems/list.html.j2")]
struct ListSystemsView {
    ctx: PageContext,
    systems: Vec<System>,
}

#[rocket::get("/systems")]
async fn list_systems(
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
) -> AppResult<ListSystemsView> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: support partial listing; ManageSystem(something)
    // (use `let systems = if all { query all } else { query some }`)

    let systems = sqlx::query_as("SELECT * FROM systems ORDER BY id")
        .fetch_all(db.inner())
        .await?;

    Ok(ListSystemsView { ctx, systems })
}
