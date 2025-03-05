use rinja::Template;
use rocket::{response::content::RawHtml, State};
use sqlx::PgPool;

use crate::{
    errors::{AppError, AppResult},
    guards::{context::PageContext, perms::PermsEvaluator},
    models::ApiToken,
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
};

use super::RenderedTemplate;

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
) -> AppResult<RenderedTemplate> {
    // TODO: redirect to system details if not partial

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

    // ensure system exists
    if api_tokens.is_empty() {
        sqlx::query("SELECT COUNT(*) FROM systems WHERE id = $1")
            .bind(system_id)
            .fetch_optional(db.inner())
            .await?
            .ok_or_else(|| AppError::NoSuchSystem(system_id.to_owned()))?;
    }

    let template = ListApiTokensView { ctx, api_tokens };

    Ok(RawHtml(template.render()?))
}
