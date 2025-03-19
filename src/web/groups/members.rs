use rinja::Template;
use rocket::{
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{GroupMember, Subgroup},
    routing::RouteTree,
    services::groups::{self, AuthorityInGroup},
    web::{Either, RenderedTemplate},
};

pub fn routes() -> RouteTree {
    rocket::routes![list_members].into()
}

#[derive(Template)]
#[template(path = "groups/members/list.html.j2")]
struct ListMembersView<'a> {
    ctx: PageContext,
    group_id: &'a str,
    group_domain: &'a str,
    subgroups: Vec<Subgroup>,
    members: Vec<GroupMember>,
    show_indirect: bool,
    can_manage: bool,
}

#[rocket::get("/groups/<domain>/<id>/members?<show_indirect>")]
#[allow(clippy::too_many_arguments)]
pub async fn list_members<'v>(
    id: &str,
    domain: &str,
    show_indirect: bool,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    if partial.is_none() {
        // we only know how to render a table, not a full page;
        // redirect to group details

        let target = uri!(super::group_details(id, domain));
        return Ok(Either::Right(Redirect::to(target)));
    }

    let authority = groups::details::require_authority(
        AuthorityInGroup::View,
        id,
        domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

    let (subgroups, members) = if show_indirect {
        (
            vec![],
            groups::members::get_all_members(id, domain, db.inner()).await?,
        )
    } else {
        (
            groups::members::get_direct_subgroups(id, domain, db.inner()).await?,
            groups::members::get_direct_members(id, domain, db.inner()).await?,
        )
    };

    let template = ListMembersView {
        ctx,
        group_id: id,
        group_domain: domain,
        subgroups,
        members,
        show_indirect,
        can_manage: authority >= AuthorityInGroup::ManageMembers,
    };

    Ok(Either::Left(RawHtml(template.render()?)))
}
