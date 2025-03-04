use askama_rocket::Template;
use rocket::State;
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, perms::PermsEvaluator},
    models::System,
    perms::HivePermission,
    routing::RouteTree,
    sanitizers::SearchTerm,
};

pub fn routes() -> RouteTree {
    rocket::routes![list_systems].into()
}

#[derive(Template)]
#[template(path = "systems/list.html.j2")]
struct ListSystemsView<'q> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
}

#[rocket::get("/systems?<q>")]
async fn list_systems<'q>(
    q: Option<&'q str>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
) -> AppResult<ListSystemsView<'q>> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: support partial listing; ManageSystem(something)
    // (use `let systems = if all { query all } else { query some }`)

    let mut query = sqlx::QueryBuilder::new("SELECT * FROM systems");

    if let Some(search) = q {
        // this will push the same bind twice even though both could be
        // references to the same $1 param... there doesn't seem to be a
        // way to avoid this, since push_bind adds $n to the query itself
        let term = SearchTerm::from(search).anywhere();
        query.push(" WHERE id ILIKE ");
        query.push_bind(term.clone());
        query.push(" OR description ILIKE ");
        query.push_bind(term);
    }

    let systems = query.build_query_as().fetch_all(db.inner()).await?;

    Ok(ListSystemsView { ctx, systems, q })
}
