use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use crate::{
    dto::permissions::AssignPermissionDto,
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{Permission, PermissionAssignment, SimpleGroup},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::groups::{self, AuthorityInGroup},
    web::{Either, RenderedTemplate},
};

pub fn routes() -> RouteTree {
    rocket::routes![list_permission_assignments, assign_permission].into()
}

#[derive(Template)]
#[template(path = "groups/permissions/list.html.j2")]
struct ListPermissionAssignmentsView {
    ctx: PageContext,
    permission_assignments: Vec<PermissionAssignment>,
    can_manage_any: bool,
}

#[derive(Template)]
#[template(
    path = "groups/permissions/assign.html.j2",
    block = "inner_assign_permission_form"
)]
struct PartialAssignPermissionView<'f, 'v> {
    ctx: PageContext,
    group: SimpleGroup,
    assignable_permissions: Vec<Permission>,
    assign_permission_form: &'f form::Context<'v>,
    assign_permission_success: Option<PermissionAssignment>,
}

#[rocket::get("/group/<domain>/<id>/permissions")]
pub async fn list_permission_assignments(
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
        groups::permissions::get_all_assignments(id, domain, db.inner(), perms).await?;

    // this could've been directly in the template, but askama doesn't seem
    // to support closures defined in the source (parsing error)
    let can_manage_any = permission_assignments
        .iter()
        .any(|a| matches!(a.can_manage, Some(true)));

    let template = ListPermissionAssignmentsView {
        ctx,
        permission_assignments,
        can_manage_any,
    };

    Ok(Either::Left(RawHtml(template.render()?)))
}

#[rocket::post("/group/<domain>/<id>/permissions", data = "<form>")]
#[allow(clippy::too_many_arguments)]
pub async fn assign_permission<'v>(
    id: &str,
    domain: &str,
    form: Form<Contextual<'v, AssignPermissionDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    let group = groups::details::require_one(id, domain, db.inner()).await?;

    let assignable_permissions = groups::permissions::get_all_assignable(perms, db.inner()).await?;

    if let Some(dto) = &form.value {
        // validation passed

        let min = HivePermission::AssignPerms(SystemsScope::Id(dto.perm.system_id.to_owned()));
        perms.require(min).await?;

        let assignment = groups::permissions::assign(id, domain, dto, db.inner(), &user).await?;

        if partial.is_some() {
            let template = PartialAssignPermissionView {
                ctx,
                assign_permission_form: &form::Context::default(),
                assign_permission_success: Some(assignment),
                group,
                assignable_permissions,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?added_permission=$key

            let target = uri!(super::group_details(id = id, domain = domain));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Assign permission form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialAssignPermissionView {
                ctx,
                assign_permission_form: &form.context,
                assign_permission_success: None,
                group,
                assignable_permissions,
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
