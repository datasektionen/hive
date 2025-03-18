use std::{
    cmp::{Ordering, Reverse},
    fmt,
};

use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form, FromFormField},
    http::Header,
    response::{content::RawHtml, Redirect},
    uri, Responder, State, UriDisplayQuery,
};
use sqlx::PgPool;

use super::{filters, GracefulRedirect, RenderedTemplate};
use crate::{
    dto::groups::EditGroupDto,
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
    rocket::routes![list_groups, group_details, delete_group, edit_group].into()
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
struct GroupDetailsView<'f, 'v> {
    ctx: PageContext,
    group: Group,
    relevance: GroupRelevance,
    edit_form: &'f form::Context<'v>,
    edit_modal_open: bool,
}

#[derive(Template)]
#[template(path = "groups/edit.html.j2", block = "inner_edit_form")]
struct PartialEditGroupView<'f, 'v> {
    ctx: PageContext,
    group: Group,
    edit_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "groups/edited.html.j2")]
struct GroupEditedView<'f, 'v> {
    ctx: PageContext,
    group: Group,
    edit_form: &'f form::Context<'v>,
    edit_modal_open: bool,
}

#[derive(FromFormField, UriDisplayQuery, PartialEq, Eq, Default)]
enum ListGroupsSort {
    #[default]
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
    let sort = sort.unwrap_or_default();
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
    let group = groups::details::require_one(id, domain, db.inner()).await?;

    let relevance = groups::details::get_relevance(id, domain, db.inner(), perms, &user)
        .await?
        .ok_or_else(|| AppError::NoSuchGroup(id.to_owned(), domain.to_owned()))?;
    // ^ technically it's a permissions problem, but this prevents enumeration

    let template = GroupDetailsView {
        ctx,
        group,
        relevance,
        edit_form: &form::Context::default(),
        edit_modal_open: false,
    };

    Ok(RawHtml(template.render()?))
}

#[rocket::delete("/group/<domain>/<id>")]
pub async fn delete_group(
    id: &str,
    domain: &str,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<GracefulRedirect> {
    groups::details::get_authority(id, domain, db.inner(), perms)
        .await?
        .require(AuthorityInGroup::FullyAuthorized)?;

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    groups::management::delete(id, domain, db.inner(), &user).await?;

    // TODO: show visual confirmation of successful delete in groups list
    Ok(GracefulRedirect::to(
        uri!(list_groups(
            None::<&str>,
            None::<ListGroupsSort>,
            None::<&str>
        )),
        partial.is_some(),
    ))
}

#[derive(Responder)]
pub enum EditGroupResponse {
    SuccessPartial(RenderedTemplate, Header<'static>, Header<'static>),
    SuccessFullPage(Redirect),
    Invalid(RenderedTemplate),
}

#[rocket::patch("/group/<domain>/<id>", data = "<form>")]
#[allow(clippy::too_many_arguments)]
pub async fn edit_group<'v>(
    id: &str,
    domain: &str,
    form: Form<Contextual<'v, EditGroupDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<EditGroupResponse> {
    let relevance = groups::details::get_relevance(id, domain, db.inner(), perms, &user)
        .await?
        .ok_or_else(|| AppError::NoSuchGroup(id.to_owned(), domain.to_owned()))?;
    // ^ technically it's a permissions problem, but this prevents enumeration

    relevance
        .authority
        .require(AuthorityInGroup::FullyAuthorized)?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        groups::management::update(id, domain, dto, db.inner(), &user).await?;

        if partial.is_some() {
            let template = GroupEditedView {
                ctx,
                group: Group {
                    id: id.to_owned(),
                    domain: domain.to_owned(),
                    name_sv: dto.name_sv.to_string(),
                    name_en: dto.name_en.to_string(),
                    description_sv: dto.description_sv.to_string(),
                    description_en: dto.description_en.to_string(),
                },
                edit_form: &form::Context::default(),
                edit_modal_open: false,
            };

            Ok(EditGroupResponse::SuccessPartial(
                RawHtml(template.render()?),
                Header::new("HX-Retarget", "#edit-group"),
                Header::new("HX-Reswap", "outerHTML"),
            ))
        } else {
            let target = uri!(group_details(id = id, domain = domain));
            Ok(EditGroupResponse::SuccessFullPage(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Edit group form errors: {:?}", &form.context);

        let group = groups::details::require_one(id, domain, db.inner()).await?;

        if partial.is_some() {
            let template = PartialEditGroupView {
                ctx,
                group,
                edit_form: &form.context,
            };

            Ok(EditGroupResponse::Invalid(RawHtml(template.render()?)))
        } else {
            let template = GroupDetailsView {
                ctx,
                group,
                relevance,
                edit_form: &form.context,
                edit_modal_open: true,
            };

            Ok(EditGroupResponse::Invalid(RawHtml(template.render()?)))
        }
    }
}
