use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::{filters, systems, Either, RenderedTemplate};
use crate::{
    dto::api_tokens::CreateApiTokenDto,
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{ActionKind, ApiToken, TargetKind},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
};

pub fn routes() -> RouteTree {
    rocket::routes![list_api_tokens, create_api_token].into()
}

#[derive(Template)]
#[template(path = "api-tokens/list.html.j2")]
struct ListApiTokensView {
    ctx: PageContext,
    api_tokens: Vec<ApiToken>,
}

#[derive(Template)]
#[template(
    path = "api-tokens/create.html.j2",
    block = "inner_create_api_token_form"
)]
struct PartialCreateApiTokenView<'f, 'v> {
    ctx: PageContext,
    api_token_create_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "api-tokens/created.html.j2")]
struct ApiTokenCreatedView<'a> {
    ctx: PageContext,
    system_id: &'a str,
    token: ApiToken,
    secret: Uuid,
}

#[derive(Template)]
#[template(
    path = "api-tokens/created.html.j2",
    block = "api_token_created_partial"
)]
struct PartialApiTokenCreatedView {
    ctx: PageContext,
    token: ApiToken,
    secret: Uuid,
}

#[rocket::get("/system/<system_id>/api-tokens")]
async fn list_api_tokens(
    system_id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    if partial.is_none() {
        // we only know how to render a table, not a full page;
        // redirect to system details

        let target = uri!(systems::system_details(system_id));
        return Ok(Either::Right(Redirect::to(target)));
    }

    perms
        .require_any_of(&[
            HivePermission::ManageSystems,
            HivePermission::ManageSystem(SystemsScope::Id(system_id.to_owned())),
        ])
        .await?;

    let api_tokens = sqlx::query_as(
        "SELECT * FROM api_tokens WHERE system_id = $1 ORDER BY expires_at, last_used_at, id",
    )
    .bind(system_id)
    .fetch_all(db.inner())
    .await?;

    if api_tokens.is_empty() {
        systems::ensure_exists(system_id, db.inner()).await?;
    }

    let template = ListApiTokensView { ctx, api_tokens };

    Ok(Either::Left(RawHtml(template.render()?)))
}

#[rocket::post("/system/<system_id>/api-tokens", data = "<form>")]
async fn create_api_token<'v>(
    system_id: &str,
    mut form: Form<Contextual<'v, CreateApiTokenDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    perms
        .require_any_of(&[
            HivePermission::ManageSystems,
            HivePermission::ManageSystem(SystemsScope::Id(system_id.to_owned())),
        ])
        .await?;

    systems::ensure_exists(system_id, db.inner()).await?;

    // TODO: anti-CSRF

    CreateApiTokenDto::extra_validation(&mut form);

    if let Some(dto) = &form.value {
        // validation passed

        let secret = Uuid::new_v4();

        let mut txn = db.begin().await?;

        let token: ApiToken = sqlx::query_as(
            "INSERT INTO api_tokens (secret, system_id, description, expires_at) VALUES ($1, $2, \
             $3, $4) RETURNING *",
        )
        .bind(secret)
        .bind(system_id)
        .bind(dto.description)
        .bind(&dto.expiration)
        .fetch_one(&mut *txn)
        .await?;

        sqlx::query(
            "INSERT INTO audit_logs (action_kind, target_kind, target_id, actor, details) VALUES \
             ($1, $2, $3, $4, $5)",
        )
        .bind(ActionKind::Create)
        .bind(TargetKind::ApiToken)
        .bind(token.id)
        .bind(user.username)
        .bind(json!({
            "new": {
                "system_id": system_id,
                "description": dto.description,
                "expires_at": dto.expiration
            }
        }))
        .execute(&mut *txn)
        .await?;

        txn.commit().await?;

        if partial.is_some() {
            let template = PartialApiTokenCreatedView { ctx, token, secret };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            let template = ApiTokenCreatedView {
                ctx,
                system_id,
                token,
                secret,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Create API token form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialCreateApiTokenView {
                ctx,
                api_token_create_form: &form.context,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(systems::system_details(system_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}
