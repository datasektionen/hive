use rinja::Template;
use rocket::{response::content::RawHtml, State};
use sqlx::PgPool;

use super::{filters, RenderedTemplate};
use crate::{
    errors::AppResult,
    guards::{
        context::PageContext, headers::HxRequest, lang::Language, perms::PermsEvaluator, user::User,
    },
    routing::RouteTree,
    services::groups::{self, GroupMembershipKind, GroupOverviewSummary, RoleInGroup},
};

pub fn routes() -> RouteTree {
    rocket::routes![list_groups].into()
}

#[derive(Template)]
#[template(path = "groups/list.html.j2")]
struct ListGroupsView<'q> {
    ctx: PageContext,
    summaries: Vec<GroupOverviewSummary>,
    q: Option<&'q str>,
}

#[derive(Template)]
#[template(path = "groups/list.html.j2", block = "inner_groups_listing")]
struct PartialListGroupsView<'q> {
    ctx: PageContext,
    summaries: Vec<GroupOverviewSummary>,
    q: Option<&'q str>,
}

#[rocket::get("/groups?<q>")]
async fn list_groups(
    q: Option<&str>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    let summaries = groups::list_summaries(q, db.inner(), perms, &user).await?;

    // TODO: order by name in correct language from ctx

    if partial.is_some() {
        let template = PartialListGroupsView { ctx, summaries, q };

        Ok(RawHtml(template.render()?))
    } else {
        let template = ListGroupsView { ctx, summaries, q };

        Ok(RawHtml(template.render()?))
    }
}
