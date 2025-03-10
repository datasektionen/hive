use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    http::Header,
    response::{content::RawHtml, Redirect},
    uri, Responder, State,
};
use sqlx::PgPool;

use super::{filters, Either, GracefulRedirect, RenderedTemplate};
use crate::{
    dto::systems::{CreateSystemDto, EditSystemDto},
    errors::{AppError, AppResult},
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::System,
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::systems,
};

pub fn routes() -> RouteTree {
    rocket::routes![
        list_systems,
        create_system,
        system_details,
        delete_system,
        edit_system
    ]
    .into()
}

#[derive(Template)]
#[template(path = "systems/list.html.j2")]
struct ListSystemsView<'q, 'f, 'v> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
    fully_authorized: bool,
    create_form: &'f form::Context<'v>,
    create_modal_open: bool,
}

// FIXME: separate Partial struct is only needed until the next Askama/Rinja
// release; after that use new attr `blocks` (feature-gated) to impl many
// methods for the same template struct
#[derive(Template)]
#[template(path = "systems/list.html.j2", block = "inner_systems_listing")]
struct PartialListSystemsView<'q> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
}

#[derive(Template)]
#[template(path = "systems/create.html.j2", block = "inner_create_form")]
struct PartialCreateSystemView<'f, 'v> {
    ctx: PageContext,
    create_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "systems/details.html.j2")]
struct SystemDetailsView<'f, 'v> {
    ctx: PageContext,
    system: System,
    fully_authorized: bool,
    can_manage_permissions: bool,
    api_token_create_form: &'f form::Context<'v>,
    permission_create_form: &'f form::Context<'v>,
    edit_form: &'f form::Context<'v>,
    edit_modal_open: bool,
}

#[derive(Template)]
#[template(path = "systems/edit.html.j2", block = "inner_edit_form")]
struct PartialEditSystemView<'f, 'v> {
    ctx: PageContext,
    system: System,
    edit_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "systems/edited.html.j2")]
struct SystemEditedView<'f, 'v> {
    ctx: PageContext,
    system: System,
    edit_form: &'f form::Context<'v>,
    edit_modal_open: bool,
}

#[rocket::get("/systems?<q>")]
async fn list_systems(
    q: Option<&str>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    let fully_authorized = perms.satisfies(HivePermission::ManageSystems).await?;

    // check against everything first, without worrying about search query
    if !fully_authorized {
        perms
            .require(HivePermission::ManageSystem(SystemsScope::Any))
            .await?;
    }

    let systems = systems::list_manageable(q, fully_authorized, db.inner(), perms).await?;

    if partial.is_some() {
        let template = PartialListSystemsView { ctx, systems, q };

        Ok(RawHtml(template.render()?))
    } else {
        let template = ListSystemsView {
            ctx,
            systems,
            q,
            fully_authorized,
            create_form: &form::Context::default(),
            create_modal_open: false,
        };

        Ok(RawHtml(template.render()?))
    }
}

#[rocket::post("/systems", data = "<form>")]
async fn create_system<'v>(
    form: Form<Contextual<'v, CreateSystemDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, GracefulRedirect>> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        systems::create_new(dto, db.inner(), &user).await?;

        Ok(Either::Right(GracefulRedirect::to(
            uri!(system_details(dto.id)),
            partial.is_some(),
        )))
    } else {
        // some errors are present; show the form again
        debug!("Create system form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialCreateSystemView {
                ctx,
                create_form: &form.context,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            let systems = systems::list_manageable(None, true, db.inner(), perms).await?;

            let template = ListSystemsView {
                ctx,
                systems,
                q: None,
                fully_authorized: true,
                create_form: &form.context,
                create_modal_open: true,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        }
    }
}

#[rocket::get("/system/<id>")]
pub async fn system_details(
    id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
) -> AppResult<RenderedTemplate> {
    let fully_authorized = perms.satisfies(HivePermission::ManageSystems).await?;

    if !fully_authorized {
        let scope = SystemsScope::Id(id.to_owned());
        perms.require(HivePermission::ManageSystem(scope)).await?;
    }

    let system = systems::get_by_id(id, db.inner())
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;
    // ^ note: there is no enumeration vulnerability in returning 404 here
    // because we already checked that the user has perms to see all systems

    let can_manage_permissions = perms
        .satisfies(HivePermission::ManagePerms(SystemsScope::Id(id.to_owned())))
        .await?;

    let empty_form = form::Context::default();

    let template = SystemDetailsView {
        ctx,
        system,
        fully_authorized,
        can_manage_permissions,
        api_token_create_form: &empty_form,
        permission_create_form: &empty_form,
        edit_form: &empty_form,
        edit_modal_open: false,
    };

    Ok(RawHtml(template.render()?))
}

#[rocket::delete("/system/<id>")]
pub async fn delete_system(
    id: &str,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<GracefulRedirect> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    systems::delete(id, db.inner(), &user).await?;

    // TODO: show visual confirmation of successful delete in systems list
    Ok(GracefulRedirect::to(
        uri!(list_systems(None::<&str>)),
        partial.is_some(),
    ))
}

#[derive(Responder)]
pub enum EditSystemResponse {
    SuccessPartial(RenderedTemplate, Header<'static>, Header<'static>),
    SuccessFullPage(Redirect),
    Invalid(RenderedTemplate),
}

#[rocket::patch("/system/<id>", data = "<form>")]
pub async fn edit_system<'v>(
    id: &str,
    form: Form<Contextual<'v, EditSystemDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<EditSystemResponse> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        systems::update(id, dto, db.inner(), &user).await?;

        if partial.is_some() {
            let template = SystemEditedView {
                ctx,
                system: System {
                    id: id.to_owned(),
                    description: dto.description.to_owned(),
                },
                edit_form: &form::Context::default(),
                edit_modal_open: false,
            };

            Ok(EditSystemResponse::SuccessPartial(
                RawHtml(template.render()?),
                Header::new("HX-Retarget", "#edit-system"),
                Header::new("HX-Reswap", "outerHTML"),
            ))
        } else {
            let target = uri!(system_details(id));
            Ok(EditSystemResponse::SuccessFullPage(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Edit system form errors: {:?}", &form.context);

        let system = systems::get_by_id(id, db.inner())
            .await?
            .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;

        if partial.is_some() {
            let template = PartialEditSystemView {
                ctx,
                system,
                edit_form: &form.context,
            };

            Ok(EditSystemResponse::Invalid(RawHtml(template.render()?)))
        } else {
            let can_manage_permissions = perms
                .satisfies(HivePermission::ManagePerms(SystemsScope::Id(id.to_owned())))
                .await?;

            let empty_form = form::Context::default();

            let template = SystemDetailsView {
                ctx,
                system,
                fully_authorized: true, // checked at the beginning of this fn
                can_manage_permissions,
                api_token_create_form: &empty_form,
                permission_create_form: &empty_form,
                edit_form: &form.context,
                edit_modal_open: true,
            };

            Ok(EditSystemResponse::Invalid(RawHtml(template.render()?)))
        }
    }
}
