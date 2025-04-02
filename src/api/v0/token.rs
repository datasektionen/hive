use rocket::{serde::json::Json, State};
use sqlx::PgPool;
use uuid::Uuid;

use super::PermKey;
use crate::{errors::AppResult, routing::RouteTree, services::permissions};

pub fn routes() -> RouteTree {
    rocket::routes![token_permissions_for_system, token_has_permission].into()
}

#[rocket::get("/token/<secret>/<system_id>")]
async fn token_permissions_for_system(
    secret: Uuid,
    system_id: &str,
    db: &State<PgPool>,
) -> AppResult<Json<Vec<String>>> {
    let perms = permissions::list_all_assignments_for_token_system(secret, system_id, db.inner())
        .await?
        .into_iter()
        .map(|assignment| match assignment.scope {
            Some(scope) => format!("{}:{}", assignment.perm_id, scope),
            None => assignment.perm_id,
        })
        .collect();

    Ok(Json(perms))
}

#[rocket::get("/token/<secret>/<system_id>/<perm_key>")]
async fn token_has_permission(
    secret: Uuid,
    system_id: &str,
    perm_key: PermKey<'_>,
    db: &State<PgPool>,
) -> AppResult<Json<bool>> {
    let has_permission = permissions::token_has_permission(
        secret,
        system_id,
        perm_key.perm_id,
        perm_key.scope,
        db.inner(),
    )
    .await?;

    Ok(Json(has_permission))
}
