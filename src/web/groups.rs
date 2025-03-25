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

use super::{filters, Either, GracefulRedirect, RenderedTemplate};
use crate::{
    dto::groups::{CreateGroupDto, EditGroupDto},
    errors::{AppError, AppResult},
    guards::{
        context::PageContext, headers::HxRequest, lang::Language, perms::PermsEvaluator, user::User,
    },
    models::{Group, GroupMember, Permission, PermissionAssignment, SimpleGroup, Subgroup},
    perms::{GroupsScope, HivePermission},
    routing::RouteTree,
    services::groups::{
        self, list::GroupOverviewSummary, AuthorityInGroup, GroupMembershipKind, GroupRelevance,
        RoleInGroup,
    },
};

mod members;
mod permissions;

pub fn routes() -> RouteTree {
    RouteTree::Branch(vec![
        rocket::routes![
            list_groups,
            create_group,
            group_details,
            delete_group,
            edit_group,
            group_info_tooltip
        ]
        .into(),
        members::routes(),
        permissions::routes(),
    ])
}

#[derive(Template)]
#[template(path = "groups/list.html.j2")]
struct ListGroupsView<'r, 'f, 'v> {
    ctx: PageContext,
    summaries: Vec<GroupOverviewSummary>,
    q: Option<&'r str>,
    sort: ListGroupsSort,
    domain_filter: Option<&'r str>,
    domains: Vec<String>,
    fully_authorized: bool,
    create_form: &'f form::Context<'v>,
    create_modal_open: bool,
}

#[derive(Template)]
#[template(path = "groups/list.html.j2", block = "inner_groups_listing")]
struct PartialListGroupsView<'q> {
    ctx: PageContext,
    summaries: Vec<GroupOverviewSummary>,
    q: Option<&'q str>,
}

#[derive(Template)]
#[template(path = "groups/create.html.j2", block = "inner_create_form")]
struct PartialCreateGroupView<'f, 'v> {
    ctx: PageContext,
    create_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "groups/details.html.j2")]
struct GroupDetailsView<'f, 'v> {
    ctx: PageContext,
    group: Group,
    relevance: GroupRelevance,
    add_subgroup_form: &'f form::Context<'v>,
    add_subgroup_success: Option<Subgroup>,
    add_member_form: &'f form::Context<'v>,
    add_member_success: Option<GroupMember>,
    assign_permission_form: &'f form::Context<'v>,
    assign_permission_success: Option<PermissionAssignment>,
    edit_form: &'f form::Context<'v>,
    edit_modal_open: bool,
    // for autocomplete
    permissible_groups: Vec<SimpleGroup>,
    assignable_permissions: Vec<Permission>,
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

#[derive(Template)]
#[template(path = "groups/info-tooltip.html.j2")]
struct GroupInfoTooltipView {
    ctx: PageContext,
    group: SimpleGroup,
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

    // unstable is faster, and we should have no equal elements anyway
    summaries.sort_unstable_by(|a, b| sort.ordering(a, b, &ctx.lang));

    let mut domains: Vec<_> = summaries.iter().map(|s| s.group.domain.clone()).collect();
    domains.sort();
    domains.dedup();

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

        let fully_authorized = perms
            .satisfies(HivePermission::ManageGroups(GroupsScope::Wildcard))
            .await?;

        let template = ListGroupsView {
            ctx,
            summaries,
            q,
            sort,
            domain_filter: domain,
            domains,
            fully_authorized,
            create_form: &form::Context::default(),
            create_modal_open: false,
        };

        Ok(RawHtml(template.render()?))
    }
}

#[rocket::post("/groups", data = "<form>")]
async fn create_group<'v>(
    form: Form<Contextual<'v, CreateGroupDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, GracefulRedirect>> {
    perms
        .require(HivePermission::ManageGroups(GroupsScope::Wildcard))
        .await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        groups::management::create(dto, db.inner(), &user).await?;

        Ok(Either::Right(GracefulRedirect::to(
            uri!(group_details(id = *dto.id, domain = *dto.domain)),
            partial.is_some(),
        )))
    } else {
        // some errors are present; show the form again
        debug!("Create group form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialCreateGroupView {
                ctx,
                create_form: &form.context,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: the list route handler is pretty complex, so we really
            // shouldn't be replicating it here... but we also want to make sure
            // that form progress is not lost

            let sort = <ListGroupsSort as Default>::default();

            let mut summaries =
                groups::list::list_summaries(None, None, db.inner(), perms, &user).await?;
            // unstable is faster, and we should have no equal elements anyway
            summaries.sort_unstable_by(|a, b| sort.ordering(a, b, &ctx.lang));

            let mut domains: Vec<_> = summaries.iter().map(|s| s.group.domain.clone()).collect();
            domains.sort();
            domains.dedup();

            let template = ListGroupsView {
                ctx,
                summaries,
                q: None,
                sort,
                domain_filter: None,
                domains,
                fully_authorized: true, // we already checked
                create_form: &form.context,
                create_modal_open: true,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        }
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

    let permissible_groups =
        groups::list::list_all_permissible_sorted(&ctx.lang, db.inner(), perms, &user).await?;

    let assignable_permissions = groups::permissions::get_all_assignable(perms, db.inner()).await?;

    let empty_form = form::Context::default();
    let template = GroupDetailsView {
        ctx,
        group,
        relevance,
        add_subgroup_form: &empty_form,
        add_subgroup_success: None,
        add_member_form: &empty_form,
        add_member_success: None,
        assign_permission_form: &empty_form,
        assign_permission_success: None,
        edit_form: &empty_form,
        edit_modal_open: false,
        permissible_groups,
        assignable_permissions,
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
    groups::details::require_authority(
        AuthorityInGroup::FullyAuthorized,
        id,
        domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

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
    groups::details::require_authority(
        AuthorityInGroup::FullyAuthorized,
        id,
        domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

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
            let relevance = groups::details::get_relevance(id, domain, db.inner(), perms, &user)
                .await?
                .ok_or_else(|| AppError::NoSuchGroup(id.to_owned(), domain.to_owned()))?;

            let permissible_groups =
                groups::list::list_all_permissible_sorted(&ctx.lang, db.inner(), perms, &user)
                    .await?;

            let assignable_permissions =
                groups::permissions::get_all_assignable(perms, db.inner()).await?;

            let empty_form = form::Context::default();
            let template = GroupDetailsView {
                ctx,
                group,
                relevance,
                add_subgroup_form: &empty_form,
                add_subgroup_success: None,
                add_member_form: &empty_form,
                add_member_success: None,
                assign_permission_form: &empty_form,
                assign_permission_success: None,
                edit_form: &form.context,
                edit_modal_open: true,
                permissible_groups,
                assignable_permissions,
            };

            Ok(EditGroupResponse::Invalid(RawHtml(template.render()?)))
        }
    }
}

#[rocket::get("/group/<domain>/<id>/tooltip")]
async fn group_info_tooltip(
    id: &str,
    domain: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    if partial.is_none() {
        // we only know how to render a tooltip, a tiny fragment, not a full
        // page - so redirect to group details

        let target = uri!(group_details(id = id, domain = domain));
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

    // no enumeration vuln because we already checked permissions
    let group = groups::details::require_one(id, domain, db.inner()).await?;

    let template = GroupInfoTooltipView { ctx, group };

    Ok(Either::Left(RawHtml(template.render()?)))
}
