use std::{
    cmp::{Ordering, Reverse},
    fmt,
};

use rinja::Template;
use rocket::{form::FromFormField, response::content::RawHtml, State};
use sqlx::PgPool;

use super::{filters, RenderedTemplate};
use crate::{
    errors::{AppError, AppResult},
    guards::{
        context::PageContext, headers::HxRequest, lang::Language, perms::PermsEvaluator, user::User,
    },
    models::Group,
    routing::RouteTree,
    services::groups::{
        self, list::GroupOverviewSummary, AuthorityInGroup, GroupMembershipKind, GroupRelevance,
        RoleInGroup,
    },
};

pub fn routes() -> RouteTree {
    rocket::routes![list_groups, group_details].into()
}

#[derive(Template)]
#[template(path = "groups/list.html.j2")]
struct ListGroupsView<'r> {
    ctx: PageContext,
    summaries: Vec<GroupOverviewSummary>,
    q: Option<&'r str>,
    sort: ListGroupsSort,
    domain_filter: Option<&'r str>,
    domains: Vec<String>,
}

#[derive(Template)]
#[template(path = "groups/list.html.j2", block = "inner_groups_listing")]
struct PartialListGroupsView<'q> {
    ctx: PageContext,
    summaries: Vec<GroupOverviewSummary>,
    q: Option<&'q str>,
}

#[derive(Template)]
#[template(path = "groups/details.html.j2")]
struct GroupDetailsView {
    ctx: PageContext,
    group: Group,
    relevance: GroupRelevance,
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

#[rocket::get("/groups?<q>&<sort>&<domain>")]
#[allow(clippy::too_many_arguments)]
async fn list_groups(
    q: Option<&str>,
    sort: Option<ListGroupsSort>,
    domain: Option<&str>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    let sort = sort.unwrap_or(ListGroupsSort::Name);
    let domain_filter = domain.map(str::to_lowercase);

    let mut summaries = groups::list::list_summaries(q, domain, db.inner(), perms, &user).await?;

    let mut domains: Vec<_> = summaries.iter().map(|s| s.group.domain.clone()).collect();
    domains.sort();
    domains.dedup();

    // unstable is faster, and we should have no equal elements anyway
    summaries.sort_unstable_by(|a, b| sort.ordering(a, b, &ctx.lang));

    if partial.is_some() {
        let template = PartialListGroupsView { ctx, summaries, q };

        Ok(RawHtml(template.render()?))
    } else {
        if let Some(filter) = domain_filter {
            // ensure current value can be shown to be selected
            if !domains.contains(&filter) {
                domains.push(filter);
            }
        }

        let template = ListGroupsView {
            ctx,
            summaries,
            q,
            sort,
            domain_filter: domain,
            domains,
        };

        Ok(RawHtml(template.render()?))
    }
}

#[rocket::get("/group/<domain>/<id>")]
async fn group_details(
    id: &str,
    domain: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
) -> AppResult<RenderedTemplate> {
    let group = groups::details::get_one(id, domain, db.inner())
        .await?
        .ok_or_else(|| AppError::NoSuchGroup(id.to_owned(), domain.to_owned()))?;

    let relevance = groups::details::get_relevance(id, domain, db.inner(), perms, &user)
        .await?
        .ok_or_else(|| AppError::NoSuchGroup(id.to_owned(), domain.to_owned()))?;
    // ^ technically it's a permissions problem, but this prevents enumeration

    let template = GroupDetailsView {
        ctx,
        group,
        relevance,
    };

    Ok(RawHtml(template.render()?))
}
