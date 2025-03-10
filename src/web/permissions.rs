use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use super::{Either, RenderedTemplate};
use crate::{
    dto::permissions::CreatePermissionDto,
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::Permission,
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::{permissions, systems},
};

pub fn routes() -> RouteTree {
    rocket::routes![list_permissions, create_permission].into()
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
#[template(path = "permissions/created.html.j2")]
struct PermissionCreatedView<'a> {
    ctx: PageContext,
    system_id: &'a str,
    permission: Permission,
}

#[derive(Template)]
#[template(
    path = "permissions/created.html.j2",
    block = "permission_created_partial"
)]
struct PartialPermissionCreatedView {
    ctx: PageContext,
    permission: Permission,
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

    let min = HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned()));
    let template = ListPermissionsView {
        ctx,
        permissions,
        can_manage: perms.satisfies(min).await?,
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
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    let min = HivePermission::ManagePerms(SystemsScope::Id(system_id.to_owned()));
    perms.require(min).await?;

    systems::ensure_exists(system_id, db.inner()).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        let permission = permissions::create_new(system_id, dto, db.inner(), &user).await?;

        if partial.is_some() {
            let template = PartialPermissionCreatedView { ctx, permission };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            let template = PermissionCreatedView {
                ctx,
                system_id,
                permission,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        }
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
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}
