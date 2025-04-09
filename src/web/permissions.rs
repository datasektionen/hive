use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;
use uuid::Uuid;

use super::{Either, GracefulRedirect, RenderedTemplate};
use crate::{
    dto::permissions::{
        AssignPermissionToApiTokenDto, AssignPermissionToGroupDto, CreatePermissionDto,
    },
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{AffiliatedPermissionAssignment, Permission},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::{permissions, systems},
};

pub fn routes() -> RouteTree {
    rocket::routes![
        list_permissions,
        create_permission,
        permission_details,
        delete_permission,
        list_permission_groups,
        list_permission_api_tokens,
        assign_permission_to_group,
        assign_permission_to_api_token,
        unassign_permission
    ]
    .into()
}

#[derive(Template)]
#[template(path = "permissions/list.html.j2")]
struct ListPermissionsView {
    ctx: PageContext,
    permissions: Vec<Permission>,
    can_manage: bool,
}

#[derive(Template)]
#[template(
    path = "permissions/create.html.j2",
    block = "inner_create_permission_form"
)]
struct PartialCreatePermissionView<'f, 'v> {
    ctx: PageContext,
    permission_create_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "permissions/details.html.j2")]
struct PermissionDetailsView<'f, 'v> {
    ctx: PageContext,
    permission: Permission,
    fully_authorized: bool,
    assign_to_group_form: &'f form::Context<'v>,
    assign_to_group_success: Option<AffiliatedPermissionAssignment>,
    assign_to_api_token_form: &'f form::Context<'v>,
    assign_to_api_token_success: Option<AffiliatedPermissionAssignment>,
}

#[derive(Template)]
#[template(path = "permissions/groups/list.html.j2")]
struct PartialListPermissionGroupsView {
    ctx: PageContext,
    has_scope: bool,
    can_manage_any: bool,
    permission_assignments: Vec<AffiliatedPermissionAssignment>,
}

#[derive(Template)]
#[template(path = "permissions/api-tokens/list.html.j2")]
struct PartialListPermissionApiTokensView {
    ctx: PageContext,
    has_scope: bool,
    can_manage_any: bool,
    permission_assignments: Vec<AffiliatedPermissionAssignment>,
}

#[derive(Template)]
#[template(
    path = "permissions/groups/assign.html.j2",
    block = "inner_assign_to_group_form"
)]
struct AssignPermissionToGroupView<'f, 'v> {
    ctx: PageContext,
    permission: Permission,
    assign_to_group_form: &'f form::Context<'v>,
    assign_to_group_success: Option<AffiliatedPermissionAssignment>,
}

#[derive(Template)]
#[template(
    path = "permissions/api-tokens/assign.html.j2",
    block = "inner_assign_to_api_token_form"
)]
struct AssignPermissionToApiTokenView<'f, 'v> {
    ctx: PageContext,
    permission: Permission,
    assign_to_api_token_form: &'f form::Context<'v>,
    assign_to_api_token_success: Option<AffiliatedPermissionAssignment>,
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

        let target = uri!(super::systems::system_details(system_id));
        return Ok(Either::Right(Redirect::to(target)));
    }

    perms
        .require_any_of(&[
            HivePermission::ManageSystems,
            HivePermission::ManageSystem(SystemsScope::Id(system_id.to_owned())),
            HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned())),
        ])
        .await?;

    let permissions = permissions::list_for_system(system_id, db.inner()).await?;

    if permissions.is_empty() {
        systems::ensure_exists(system_id, db.inner()).await?;
    }

    let template = ListPermissionsView {
        ctx,
        permissions,
        can_manage: perms
            .satisfies_any_of(&[
                HivePermission::AssignPerms(SystemsScope::Id(system_id.to_owned())),
                HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned())),
            ])
            .await?,
    };

    Ok(Either::Left(RawHtml(template.render()?)))
}

#[rocket::post("/system/<system_id>/permissions", data = "<form>")]
async fn create_permission<'v>(
    system_id: &str,
    form: Form<Contextual<'v, CreatePermissionDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, GracefulRedirect>> {
    let min = HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned()));
    perms.require(min).await?;

    systems::ensure_exists(system_id, db.inner()).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        let permission = permissions::create_new(system_id, dto, db.inner(), &user).await?;

        Ok(Either::Right(GracefulRedirect::to(
            uri!(permission_details(
                system_id = system_id,
                perm_id = permission.perm_id
            )),
            partial.is_some(),
        )))
    } else {
        // some errors are present; show the form again
        debug!("Create permission form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialCreatePermissionView {
                ctx,
                permission_create_form: &form.context,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(super::systems::system_details(system_id));
            Ok(Either::Right(GracefulRedirect::to(target, false)))
        }
    }
}

#[rocket::get("/system/<system_id>/permission/<perm_id>")]
async fn permission_details(
    system_id: &str,
    perm_id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
) -> AppResult<RenderedTemplate> {
    let possibilities = [
        HivePermission::AssignPerms(SystemsScope::Id(system_id.to_owned())),
        HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned())),
    ];

    perms.require_any_of(&possibilities).await?;

    let permission = permissions::require_one(system_id, perm_id, db.inner()).await?;

    let empty_form = form::Context::default();
    let min = possibilities.into_iter().last().unwrap();
    let template = PermissionDetailsView {
        ctx,
        permission,
        fully_authorized: perms.satisfies(min).await?,
        assign_to_group_form: &empty_form,
        assign_to_group_success: None,
        assign_to_api_token_form: &empty_form,
        assign_to_api_token_success: None,
    };

    Ok(RawHtml(template.render()?))
}

#[rocket::delete("/system/<system_id>/permission/<perm_id>")]
pub async fn delete_permission(
    system_id: &str,
    perm_id: &str,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<GracefulRedirect> {
    let min = HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned()));
    perms.require(min).await?;

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    permissions::delete(system_id, perm_id, db.inner(), &user).await?;

    // TODO: show visual confirmation of successful delete in permissions list
    Ok(GracefulRedirect::to(
        uri!(super::systems::system_details(system_id)),
        partial.is_some(),
    ))
}

macro_rules! list_permission_assignments {
    ($path:expr, $fname:ident, $lister:path, $template:ident) => {
        #[rocket::get($path)]
        async fn $fname(
            system_id: &str,
            perm_id: &str,
            db: &State<PgPool>,
            ctx: PageContext,
            perms: &PermsEvaluator,
            partial: Option<HxRequest<'_>>,
        ) -> AppResult<Either<RenderedTemplate, Redirect>> {
            if partial.is_none() {
                // we only know how to render a table, not a full page;
                // redirect to permission details

                #[allow(clippy::redundant_locals)] // unclear why necessary
                let target = uri!(permission_details(system_id = system_id, perm_id = perm_id));
                return Ok(Either::Right(Redirect::to(target)));
            }

            perms
                .require_any_of(&[
                    HivePermission::AssignPerms(SystemsScope::Id(system_id.to_owned())),
                    HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned())),
                ])
                .await?;

            let has_scope = permissions::has_scope(system_id, perm_id, db.inner()).await?;

            let permission_assignments =
                $lister(system_id, perm_id, Some(&ctx.lang), db.inner(), perms).await?;

            // this could've been directly in the template, but askama doesn't seem
            // to support closures defined in the source (parsing error)
            let can_manage_any = permission_assignments
                .iter()
                .any(|a| matches!(a.can_manage, Some(true)));

            let template = $template {
                ctx,
                has_scope,
                can_manage_any,
                permission_assignments,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        }
    };
}

list_permission_assignments!(
    "/system/<system_id>/permission/<perm_id>/groups",
    list_permission_groups,
    permissions::list_group_assignments,
    PartialListPermissionGroupsView
);

list_permission_assignments!(
    "/system/<system_id>/permission/<perm_id>/api-tokens",
    list_permission_api_tokens,
    permissions::list_api_token_assignments,
    PartialListPermissionApiTokensView
);

#[rocket::post("/system/<system_id>/permission/<perm_id>/groups", data = "<form>")]
#[allow(clippy::too_many_arguments)]
async fn assign_permission_to_group<'v>(
    system_id: &str,
    perm_id: &str,
    form: Form<Contextual<'v, AssignPermissionToGroupDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    let min = HivePermission::AssignPerms(SystemsScope::Id(system_id.to_string()));
    perms.require(min).await?;

    // TODO: anti-CSRF

    let permission = permissions::require_one(system_id, perm_id, db.inner()).await?;

    if let Some(dto) = &form.value {
        // validation passed

        let assignment = permissions::assign_to_group(
            system_id,
            perm_id,
            dto,
            Some(&ctx.lang),
            db.inner(),
            &user,
        )
        .await?;

        if partial.is_some() {
            let template = AssignPermissionToGroupView {
                ctx,
                permission,
                assign_to_group_form: &form::Context::default(),
                assign_to_group_success: Some(assignment),
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?assigned_to_group=id@domain

            let target = uri!(permission_details(system_id = system_id, perm_id = perm_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!(
            "Assign permission to group form errors: {:?}",
            &form.context
        );

        if partial.is_some() {
            let template = AssignPermissionToGroupView {
                ctx,
                permission,
                assign_to_group_form: &form.context,
                assign_to_group_success: None,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(permission_details(system_id = system_id, perm_id = perm_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}

#[rocket::post("/system/<system_id>/permission/<perm_id>/api-tokens", data = "<form>")]
#[allow(clippy::too_many_arguments)]
async fn assign_permission_to_api_token<'v>(
    system_id: &str,
    perm_id: &str,
    form: Form<Contextual<'v, AssignPermissionToApiTokenDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    let min = HivePermission::AssignPerms(SystemsScope::Id(system_id.to_string()));
    perms.require(min).await?;

    // TODO: anti-CSRF

    let permission = permissions::require_one(system_id, perm_id, db.inner()).await?;

    if let Some(dto) = &form.value {
        // validation passed

        let assignment = permissions::assign_to_api_token(
            system_id,
            perm_id,
            dto,
            Some(&ctx.lang),
            db.inner(),
            &user,
        )
        .await?;

        if partial.is_some() {
            let template = AssignPermissionToApiTokenView {
                ctx,
                permission,
                assign_to_api_token_form: &form::Context::default(),
                assign_to_api_token_success: Some(assignment),
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?assigned_to_api_token=id

            let target = uri!(permission_details(system_id = system_id, perm_id = perm_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!(
            "Assign permission to API token form errors: {:?}",
            &form.context
        );

        if partial.is_some() {
            let template = AssignPermissionToApiTokenView {
                ctx,
                permission,
                assign_to_api_token_form: &form.context,
                assign_to_api_token_success: None,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(permission_details(system_id = system_id, perm_id = perm_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}

#[rocket::delete("/permission-assignment/<id>")]
async fn unassign_permission(
    id: Uuid,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<(), Redirect>> {
    // perms can only be checked later, not enough info now

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    let old = permissions::unassign(id, db.inner(), perms, &user).await?;

    if partial.is_some() {
        Ok(Either::Left(()))
    } else {
        let target = uri!(permission_details(
            system_id = old.system_id,
            perm_id = old.perm_id
        ));
        Ok(Either::Right(Redirect::to(target)))
    }
}
