use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    http::Header,
    response::{content::RawHtml, Redirect},
    uri, Responder, State,
};
use serde_json::json;
use sqlx::PgPool;

use super::{filters, Either, GracefulRedirect, RenderedTemplate};
use crate::{
    dto::systems::{CreateSystemDto, EditSystemDto},
    errors::{AppError, AppResult},
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{ActionKind, System, TargetKind},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    sanitizers::SearchTerm,
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
    api_token_create_form: &'f form::Context<'v>,
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
struct SystemEditedView<'a> {
    description: &'a str,
}

#[rocket::get("/systems?<q>")]
async fn list_systems(
    q: Option<&str>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: support partial listing; ManageSystem(something)
    // (use `let systems = if all { query all } else { query some }`)

    let mut query = sqlx::QueryBuilder::new("SELECT * FROM systems");

    if let Some(search) = q {
        // this will push the same bind twice even though both could be
        // references to the same $1 param... there doesn't seem to be a
        // way to avoid this, since push_bind adds $n to the query itself
        let term = SearchTerm::from(search).anywhere();
        query.push(" WHERE id ILIKE ");
        query.push_bind(term.clone());
        query.push(" OR description ILIKE ");
        query.push_bind(term);
    }

    let systems = query.build_query_as().fetch_all(db.inner()).await?;

    if partial.is_some() {
        let template = PartialListSystemsView { ctx, systems, q };

        Ok(RawHtml(template.render()?))
    } else {
        let template = ListSystemsView {
            ctx,
            systems,
            q,
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

        let mut txn = db.begin().await?;

        let id: String = sqlx::query_scalar(
            "INSERT INTO systems (id, description) VALUES ($1, $2) RETURNING id",
        )
        .bind(dto.id)
        .bind(dto.description)
        .fetch_one(&mut *txn)
        .await?;

        sqlx::query(
            "INSERT INTO audit_logs (action_kind, target_kind, target_id, actor, details) VALUES \
             ($1, $2, $3, $4, $5)",
        )
        .bind(ActionKind::Create)
        .bind(TargetKind::System)
        .bind(&id)
        .bind(user.username)
        .bind(json!({"new": {"description": dto.description}}))
        .execute(&mut *txn)
        .await?;

        txn.commit().await?;

        Ok(Either::Right(GracefulRedirect::to(
            uri!(system_details(id)),
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
            let systems = sqlx::query_as("SELECT * FROM systems ORDER BY id")
                .fetch_all(db.inner())
                .await?;

            let template = ListSystemsView {
                ctx,
                systems,
                q: None,
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

    let system = sqlx::query_as("SELECT * FROM systems WHERE id = $1")
        .bind(id)
        .fetch_optional(db.inner())
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;
    // ^ note: there is no enumeration vulnerability in returning 404 here
    // because we already checked that the user has perms to see all systems

    let empty_form = form::Context::default();

    let template = SystemDetailsView {
        ctx,
        system,
        fully_authorized,
        api_token_create_form: &empty_form,
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

    if id == "hive" {
        // shouldn't delete ourselves
        warn!("Disallowing self-deletion from {}", user.username);
        return Err(AppError::SelfPreservation);
    }

    let mut txn = db.begin().await?;

    let system: System = sqlx::query_as("DELETE FROM systems WHERE id = $1 RETURNING *")
        .bind(id)
        .fetch_optional(&mut *txn)
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;

    sqlx::query(
        "INSERT INTO audit_logs (action_kind, target_kind, target_id, actor, details) VALUES ($1, \
         $2, $3, $4, $5)",
    )
    .bind(ActionKind::Delete)
    .bind(TargetKind::System)
    .bind(system.id)
    .bind(user.username)
    .bind(json!({"old": {"description": system.description}}))
    .execute(&mut *txn)
    .await?;

    txn.commit().await?;

    Ok(GracefulRedirect::to(
        uri!(list_systems(None::<&str>)),
        partial.is_some(),
    ))
}

#[derive(Responder)]
pub enum EditSystemResponse<'a> {
    SuccessPartial(RenderedTemplate, Header<'a>),
    SuccessFullPage(Redirect),
    Invalid(RenderedTemplate),
}

#[rocket::patch("/system/<id>", data = "<form>")]
pub async fn edit_system<'r, 'v>(
    id: &str,
    form: Form<Contextual<'v, EditSystemDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<EditSystemResponse<'r>> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        let mut txn = db.begin().await?;

        // subquery runs before update
        let old_description: String = sqlx::query_scalar(
            "UPDATE systems SET description = $1 WHERE id = $2 RETURNING (SELECT description FROM \
             systems WHERE id = $2)",
        )
        .bind(dto.description)
        .bind(id)
        .fetch_optional(&mut *txn)
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;

        if dto.description != old_description {
            sqlx::query(
                "INSERT INTO audit_logs (action_kind, target_kind, target_id, actor, details) \
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(ActionKind::Update)
            .bind(TargetKind::System)
            .bind(id)
            .bind(user.username)
            .bind(json!({
                "old": {"description": old_description},
                "new": {"description": dto.description},
            }))
            .execute(&mut *txn)
            .await?;

            txn.commit().await?;
        }

        if partial.is_some() {
            let template = SystemEditedView {
                description: dto.description,
            };

            let header = Header::new("HX-Reswap", "none");
            Ok(EditSystemResponse::SuccessPartial(
                RawHtml(template.render()?),
                header,
            ))
        } else {
            let target = uri!(system_details(id));
            Ok(EditSystemResponse::SuccessFullPage(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Edit system form errors: {:?}", &form.context);

        let system = sqlx::query_as("SELECT * FROM systems WHERE id = $1")
            .bind(id)
            .fetch_optional(db.inner())
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
            let template = SystemDetailsView {
                ctx,
                system,
                fully_authorized: true,
                api_token_create_form: &form::Context::default(),
                edit_form: &form.context,
                edit_modal_open: true,
            };

            Ok(EditSystemResponse::Invalid(RawHtml(template.render()?)))
        }
    }
}

pub async fn ensure_exists<'a, X>(id: &str, db: X) -> AppResult<()>
where
    X: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    sqlx::query("SELECT COUNT(*) FROM systems WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;

    Ok(())
}
