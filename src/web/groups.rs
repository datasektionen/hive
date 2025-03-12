use std::{
    cmp::{Ordering, Reverse},
    fmt,
};

use rinja::Template;
use rocket::{form::FromFormField, response::content::RawHtml, State};
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
    sort: ListGroupsSort,
}

#[derive(Template)]
#[template(path = "groups/list.html.j2", block = "inner_groups_listing")]
struct PartialListGroupsView<'q> {
    ctx: PageContext,
    summaries: Vec<GroupOverviewSummary>,
    q: Option<&'q str>,
}

#[derive(FromFormField, PartialEq, Eq)]
enum ListGroupsSort {
    Name,
    Id,
    Domain,
    #[field(value = "direct_members")]
    DirectMembers,
    #[field(value = "total_members")]
    TotalMembers,
}

impl fmt::Display for ListGroupsSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Name => write!(f, "name"),
            Self::Id => write!(f, "id"),
            Self::Domain => write!(f, "domain"),
            Self::DirectMembers => write!(f, "direct_members"),
            Self::TotalMembers => write!(f, "total_members"),
        }
    }
}

impl ListGroupsSort {
    fn ordering(
        &self,
        a: &GroupOverviewSummary,
        b: &GroupOverviewSummary,
        lang: &Language,
    ) -> Ordering {
        let (a_name, b_name) = match lang {
            Language::Swedish => (&a.group.name_sv, &b.group.name_sv),
            Language::English => (&a.group.name_en, &b.group.name_en),
        };

        match self {
            Self::Name => {
                (a_name, &a.group.id, &a.group.domain).cmp(&(b_name, &b.group.id, &b.group.domain))
            }
            Self::Id => (&a.group.id, &a.group.domain).cmp(&(&b.group.id, &b.group.domain)),
            Self::Domain => {
                (&a.group.domain, a_name, &a.group.id).cmp(&(&b.group.domain, b_name, &b.group.id))
            }
            Self::DirectMembers => (
                Reverse(a.n_direct_members),
                a_name,
                &a.group.id,
                &a.group.domain,
            )
                .cmp(&(
                    Reverse(b.n_direct_members),
                    b_name,
                    &b.group.id,
                    &b.group.domain,
                )),
            Self::TotalMembers => (
                Reverse(a.n_total_members),
                a_name,
                &a.group.id,
                &a.group.domain,
            )
                .cmp(&(
                    Reverse(b.n_total_members),
                    b_name,
                    &b.group.id,
                    &b.group.domain,
                )),
        }
    }
}

#[rocket::get("/groups?<q>&<sort>")]
async fn list_groups(
    q: Option<&str>,
    sort: Option<ListGroupsSort>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    let sort = sort.unwrap_or(ListGroupsSort::Name);

    let mut summaries = groups::list_summaries(q, db.inner(), perms, &user).await?;

    // unstable is faster, and we should have no equal elements anyway
    summaries.sort_unstable_by(|a, b| sort.ordering(a, b, &ctx.lang));

    if partial.is_some() {
        let template = PartialListGroupsView { ctx, summaries, q };

        Ok(RawHtml(template.render()?))
    } else {
        let template = ListGroupsView {
            ctx,
            summaries,
            q,
            sort,
        };

        Ok(RawHtml(template.render()?))
    }
}
