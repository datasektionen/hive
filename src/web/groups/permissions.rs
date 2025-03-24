use rinja::Template;
use rocket::{
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::PermissionAssignment,
    routing::RouteTree,
    services::groups::{self, AuthorityInGroup},
    web::{Either, RenderedTemplate},
};

pub fn routes() -> RouteTree {
    rocket::routes![list_permission_assignments].into()
}

#[derive(Template)]
#[template(path = "groups/permissions/list.html.j2")]
struct ListPermissionAssignmentsView {
    ctx: PageContext,
    permission_assignments: Vec<PermissionAssignment>,
}

#[rocket::get("/group/<domain>/<id>/permissions")]
pub async fn list_permission_assignments<'v>(
    id: &str,
    domain: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    if partial.is_none() {
        // we only know how to render a table, not a full page;
        // redirect to group details

        let target = uri!(super::group_details(id = id, domain = domain));
        return Ok(Either::Right(Redirect::to(target)));
    }

    groups::details::require_authority(
        AuthorityInGroup::View,
        id,
        domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

    let permission_assignments =
        groups::permissions::get_all_assignments(id, domain, db.inner()).await?;

    let template = ListPermissionAssignmentsView {
        ctx,
        permission_assignments,
    };

    Ok(Either::Left(RawHtml(template.render()?)))
}
