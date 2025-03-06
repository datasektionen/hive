use rinja::Template;
use rocket::{
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator},
    models::ApiToken,
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
};

use super::{filters, systems, RenderedTemplate};

pub fn routes() -> RouteTree {
    rocket::routes![list_api_tokens].into()
}

#[derive(Template)]
#[template(path = "api-tokens/list.html.j2")]
struct ListApiTokensView {
    ctx: PageContext,
    api_tokens: Vec<ApiToken>,
}

#[rocket::get("/system/<system_id>/api-tokens")]
async fn list_api_tokens(
    system_id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Result<RenderedTemplate, Redirect>> {
    // note on return type: the inner Result is just to make it
    // easier to have 2 possible response types without defining
    // a separate enum just for this
    // See: https://rocket.rs/guide/v0.5/faq/#multiple-responses

    if partial.is_none() {
        // we only know how to render a table, not a full page;
        // redirect to system details

        let target = uri!(super::systems::system_details(system_id));
        return Ok(Err(Redirect::to(target)));
    }

    perms
        .require_any_of(&[
            HivePermission::ManageSystems,
            HivePermission::ManageSystem(SystemsScope::Id(system_id.to_owned())),
        ])
        .await?;

    let api_tokens = sqlx::query_as(
        "SELECT * FROM api_tokens WHERE system_id = $1 ORDER BY expires_at, last_used_at, id",
    )
    .bind(system_id)
    .fetch_all(db.inner())
    .await?;

    if api_tokens.is_empty() {
        systems::ensure_exists(system_id, db.inner()).await?;
    }

    let template = ListApiTokensView { ctx, api_tokens };

    Ok(Ok(RawHtml(template.render()?)))
}
