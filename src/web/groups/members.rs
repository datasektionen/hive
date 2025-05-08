use chrono::{Datelike, Local, NaiveDate};
use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    dto::groups::{AddMemberDto, AddSubgroupDto},
    errors::{AppError, AppResult},
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{GroupMember, GroupRef, SimpleGroup, Subgroup},
    perms::{HivePermission, UpperBoundScope},
    routing::RouteTree,
    services::groups::{self, AuthorityInGroup},
    web::{Either, RenderedTemplate},
};

pub fn routes() -> RouteTree {
    rocket::routes![
        list_members,
        add_subgroup,
        add_member,
        remove_subgroup,
        remove_member,
        get_membership_details
    ]
    .into()
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

#[derive(Template)]
#[template(
    path = "groups/members/add-subgroup.html.j2",
    block = "inner_add_subgroup_form"
)]
struct PartialAddSubgroupView<'f, 'v> {
    ctx: PageContext,
    add_subgroup_form: &'f form::Context<'v>,
    add_subgroup_success: Option<Subgroup>,
    group: SimpleGroup,
    permissible_groups: Vec<SimpleGroup>,
}

#[derive(Template)]
#[template(
    path = "groups/members/add-member.html.j2",
    block = "inner_add_member_form"
)]
struct PartialAddMemberView<'r, 'f, 'v> {
    ctx: PageContext,
    group_id: &'r str,
    group_domain: &'r str,
    add_member_form: &'f form::Context<'v>,
    add_member_success: Option<GroupMember>,
}

#[derive(Template)]
#[template(path = "groups/members/member-details.html.j2")]
struct PartialMembershipDetailsView<'r> {
    ctx: PageContext,
    username: &'r str,
    paths: Vec<Vec<GroupRef>>, // empty => not indirect member (but not <=!)
    is_direct_member: bool,    // false doesn't mean indirect! might be none
}

#[rocket::get("/group/<domain>/<id>/members?<show_indirect>")]
#[allow(clippy::too_many_arguments)]
pub async fn list_members(
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

        let target = uri!(super::group_details(id = id, domain = domain));
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

#[rocket::post("/group/<domain>/<id>/subgroups", data = "<form>")]
#[allow(clippy::too_many_arguments)]
async fn add_subgroup<'v>(
    id: &str,
    domain: &str,
    form: Form<Contextual<'v, AddSubgroupDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    groups::details::require_authority(
        AuthorityInGroup::ManageMembers,
        id,
        domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

    // TODO: anti-CSRF

    let permissible_groups =
        groups::list::list_all_permissible_sorted(&ctx.lang, db.inner(), perms, &user).await?;

    let group = permissible_groups
        .iter()
        .find(|g| g.id == id && g.domain == domain)
        .expect("parent should be permissible")
        .clone();
    // ^ panic should be unreachable, we already checked permissions

    if let Some(dto) = &form.value {
        // validation passed

        groups::details::require_authority(
            AuthorityInGroup::View,
            dto.child.id,
            dto.child.domain,
            db.inner(),
            perms,
            &user,
        )
        .await?;

        groups::members::add_subgroup(id, domain, dto, db.inner(), &user).await?;

        if partial.is_some() {
            let added = permissible_groups
                .iter()
                .find(|g| g.id == dto.child.id && g.domain == dto.child.domain)
                .expect("added should be permissible")
                .clone();
            // ^ panic should be unreachable, we already checked permissions

            let template = PartialAddSubgroupView {
                ctx,
                add_subgroup_form: &form::Context::default(),
                add_subgroup_success: Some(Subgroup {
                    manager: dto.manager,
                    group: added,
                }),
                group,
                permissible_groups,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?added_subgroup=id@domain

            let target = uri!(super::group_details(id = id, domain = domain));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Add subgroup form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialAddSubgroupView {
                ctx,
                add_subgroup_form: &form.context,
                add_subgroup_success: None,
                group,
                permissible_groups,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(super::group_details(id = id, domain = domain));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}

#[rocket::post("/group/<domain>/<id>/members", data = "<form>")]
#[allow(clippy::too_many_arguments)]
async fn add_member<'v>(
    id: &str,
    domain: &str,
    mut form: Form<Contextual<'v, AddMemberDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    groups::details::require_authority(
        AuthorityInGroup::ManageMembers,
        id,
        domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

    // TODO: anti-CSRF

    if let Some(until) = form.value.as_ref().map(|dto| dto.until.0) {
        // the default limit for membership upper bound is either 31/Dec of the current year or
        // 30/Jun of the following year, whichever is closer but more than 6 months away
        let today = Local::now().date_naive();
        let limit = if today < NaiveDate::from_ymd_opt(today.year(), 6, 30).unwrap() {
            NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap()
        } else {
            NaiveDate::from_ymd_opt(today.year() + 1, 6, 30).unwrap()
        };

        if until > limit {
            // outside of base case, so need special permission

            let years_diff = until.year() - today.year();
            let months_diff = until.month() as i32 - today.month() as i32;
            let mut total_months = years_diff * 12 + months_diff;
            if until.day() > today.day() {
                total_months += 1; // adjust rounding up
            }
            let total_months = total_months.clamp(0, u8::MAX as _) as u8;

            let min = HivePermission::LongTermAppointment(UpperBoundScope::UpTo(total_months));
            if !perms.satisfies(min).await? {
                // ok, not authorized (but 403 would be confusing, so we forge a form error)
                let error = form::Error::validation("Too far in the future").with_name("until");
                form.context.push_error(error);
                form.value = None;
            }
        }
    }

    if let Some(dto) = &form.value {
        // validation passed

        let added = groups::members::add_member(id, domain, dto, db.inner(), &user).await?;

        if partial.is_some() {
            let template = PartialAddMemberView {
                ctx,
                group_id: id,
                group_domain: domain,
                add_member_form: &form::Context::default(),
                add_member_success: Some(added),
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?added_member=id@domain

            let target = uri!(super::group_details(id = id, domain = domain));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Add member form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialAddMemberView {
                ctx,
                group_id: id,
                group_domain: domain,
                add_member_form: &form.context,
                add_member_success: None,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(super::group_details(id = id, domain = domain));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}

#[rocket::delete("/group/<parent_domain>/<parent_id>/subgroup/<child_domain>/<child_id>")]
#[allow(clippy::too_many_arguments)]
async fn remove_subgroup<'v>(
    parent_id: &str,
    parent_domain: &str,
    child_id: &str,
    child_domain: &str,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<(), Redirect>> {
    groups::details::require_authority(
        AuthorityInGroup::ManageMembers,
        parent_id,
        parent_domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    groups::members::remove_subgroup(
        parent_id,
        parent_domain,
        child_id,
        child_domain,
        db.inner(),
        &user,
    )
    .await?;

    if partial.is_some() {
        Ok(Either::Left(()))
    } else {
        let target = uri!(super::group_details(id = parent_id, domain = parent_domain));
        Ok(Either::Right(Redirect::to(target)))
    }
}

#[rocket::delete("/group-membership/<id>")]
async fn remove_member<'v>(
    id: Uuid,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<(), Redirect>> {
    // cannot check perms yet, with only this information

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    let (group_id, group_domain) = groups::members::get_membership_group(&id, db.inner())
        .await?
        .ok_or_else(|| AppError::InsufficientAuthorityInGroup(AuthorityInGroup::ManageMembers))?;
    // ^ not really true, the membership doesn't exist, but we want to prevent
    // enumeration

    groups::details::require_authority(
        AuthorityInGroup::ManageMembers,
        group_id.as_str(),
        group_domain.as_str(),
        db.inner(),
        perms,
        &user,
    )
    .await?;

    groups::members::remove_member(
        &id,
        group_id.as_str(),
        group_domain.as_str(),
        db.inner(),
        &user,
    )
    .await?;

    if partial.is_some() {
        Ok(Either::Left(()))
    } else {
        let target = uri!(super::group_details(id = group_id, domain = group_domain));
        Ok(Either::Right(Redirect::to(target)))
    }
}

#[rocket::get("/group/<domain>/<id>/member/<username>")]
#[allow(clippy::too_many_arguments)]
pub async fn get_membership_details(
    id: &str,
    domain: &str,
    username: &str,
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

    let mut paths = groups::details::get_role_in_group_with_paths(username, id, domain, db.inner())
        .await?
        .1;

    let mut is_direct_member = false;
    paths.retain(|path| {
        if path.is_empty() {
            is_direct_member = true;

            false
        } else {
            true
        }
    });

    let template = PartialMembershipDetailsView {
        ctx,
        username,
        paths,
        is_direct_member,
    };

    Ok(Either::Left(RawHtml(template.render()?)))
}
