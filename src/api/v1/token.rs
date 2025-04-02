use std::collections::BTreeSet;

use rocket::{serde::json::Json, State};
use sqlx::PgPool;
use uuid::Uuid;

use super::SystemPermissionAssignment;
use crate::{
    api::HiveApiPermission, errors::AppResult, guards::api::consumer::ApiConsumer,
    routing::RouteTree, services::permissions,
};

pub fn routes() -> RouteTree {
    rocket::routes![
        token_permissions,
        token_permission_scopes,
        token_has_permission,
        token_has_permission_scoped
    ]
    .into()
}

#[rocket::get("/token/<secret>/permissions")]
async fn token_permissions(
    secret: Uuid,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<SystemPermissionAssignment>>> {
    consumer
        .require(HiveApiPermission::CheckPermissions, db.inner())
        .await?;

    let perms =
        permissions::list_all_assignments_for_token_system(secret, &consumer.system_id, db.inner())
            .await?
            .into_iter()
            .map(Into::into)
            .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(perms))
}

#[rocket::get("/token/<secret>/permission/<perm_id>/scopes")]
async fn token_permission_scopes(
    secret: Uuid,
    perm_id: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<String>>> {
    consumer
        .require(HiveApiPermission::CheckPermissions, db.inner())
        .await?;

    let scopes = permissions::list_all_scopes_for_token_permission(
        secret,
        perm_id,
        &consumer.system_id,
        db.inner(),
    )
    .await?;

    // BTreeSet orders and removes duplicates
    Ok(Json(BTreeSet::from_iter(scopes)))
}

#[rocket::get("/token/<secret>/permission/<perm_id>")]
async fn token_has_permission(
    secret: Uuid,
    perm_id: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<bool>> {
    consumer
        .require(HiveApiPermission::CheckPermissions, db.inner())
        .await?;

    let has_permission =
        permissions::token_has_permission(secret, &consumer.system_id, perm_id, None, db.inner())
            .await?;

    Ok(Json(has_permission))
}

#[rocket::get("/token/<secret>/permission/<perm_id>/scope/<scope>")]
async fn token_has_permission_scoped(
    secret: Uuid,
    perm_id: &str,
    scope: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<bool>> {
    consumer
        .require(HiveApiPermission::CheckPermissions, db.inner())
        .await?;

    let has_permission = permissions::token_has_permission(
        secret,
        &consumer.system_id,
        perm_id,
        Some(scope),
        db.inner(),
    )
    .await?;

    Ok(Json(has_permission))
}
