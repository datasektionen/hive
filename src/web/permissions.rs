use rinja::Template;
use rocket::{
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator},
    models::Permission,
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
};

use super::{systems, Either, RenderedTemplate};

pub fn routes() -> RouteTree {
    rocket::routes![list_permissions].into()
}

#[derive(Template)]
#[template(path = "permissions/list.html.j2")]
struct ListPermissionsView {
    ctx: PageContext,
    permissions: Vec<Permission>,
}

#[rocket::get("/system/<system_id>/permissions")]
async fn list_permissions(
    system_id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    if partial.is_none() {
        // we only know how to render a table, not a full page;
        // redirect to system details

        let target = uri!(systems::system_details(system_id));
        return Ok(Either::Right(Redirect::to(target)));
    }

    perms
        .require_any_of(&[
            HivePermission::ManageSystems,
            HivePermission::ManageSystem(SystemsScope::Id(system_id.to_owned())),
        ])
        .await?;

    let permissions =
        sqlx::query_as("SELECT * FROM permissions WHERE system_id = $1 ORDER BY perm_id")
            .bind(system_id)
            .fetch_all(db.inner())
            .await?;

    if permissions.is_empty() {
        systems::ensure_exists(system_id, db.inner()).await?;
    }

    let template = ListPermissionsView { ctx, permissions };

    Ok(Either::Left(RawHtml(template.render()?)))
}
