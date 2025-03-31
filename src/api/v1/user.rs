use rocket::{serde::json::Json, State};
use sqlx::PgPool;

use crate::{
    errors::AppResult, guards::api::consumer::ApiConsumer, routing::RouteTree,
    services::permissions,
};

use super::SystemPermissionAssignment;

pub fn routes() -> RouteTree {
    rocket::routes![
        user_permissions,
        user_permission_scopes,
        user_has_permission,
        user_has_permission_scoped
    ]
    .into()
}

#[rocket::get("/user/<username>/permissions")]
async fn user_permissions(
    username: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<Vec<SystemPermissionAssignment>>> {
    let perms = permissions::list_all_assignments_for_user_system(
        username,
        &consumer.system_id,
        db.inner(),
    )
    .await?
    .into_iter()
    .map(Into::into)
    .collect();

    Ok(Json(perms))
}

#[rocket::get("/user/<username>/permission/<perm_id>/scopes")]
async fn user_permission_scopes(
    username: &str,
    perm_id: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<Vec<String>>> {
    let scopes = permissions::list_all_scopes_for_user_permission(
        username,
        perm_id,
        &consumer.system_id,
        db.inner(),
    )
    .await?;

    Ok(Json(scopes))
}

#[rocket::get("/user/<username>/permission/<perm_id>")]
async fn user_has_permission(
    username: &str,
    perm_id: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<bool>> {
    let has_permission =
        permissions::user_has_permission(username, &consumer.system_id, perm_id, None, db.inner())
            .await?;

    Ok(Json(has_permission))
}

#[rocket::get("/user/<username>/permission/<perm_id>/scope/<scope>")]
async fn user_has_permission_scoped(
    username: &str,
    perm_id: &str,
    scope: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<bool>> {
    let has_permission = permissions::user_has_permission(
        username,
        &consumer.system_id,
        perm_id,
        Some(scope),
        db.inner(),
    )
    .await?;

    Ok(Json(has_permission))
}
