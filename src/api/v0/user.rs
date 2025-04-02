use std::collections::HashMap;

use rocket::{serde::json::Json, State};
use sqlx::PgPool;

use super::PermKey;
use crate::{errors::AppResult, routing::RouteTree, services::permissions};

pub fn routes() -> RouteTree {
    rocket::routes![
        user_systems,
        user_permissions_for_system,
        user_has_permission
    ]
    .into()
}

type SystemPermissionsMap = HashMap<String, Vec<String>>;

#[rocket::get("/user/<username>")]
async fn user_systems(username: &str, db: &State<PgPool>) -> AppResult<Json<SystemPermissionsMap>> {
    let assignments = permissions::list_all_assignments_for_user(username, db.inner()).await?;

    let mut map: SystemPermissionsMap = HashMap::new();

    for assignment in assignments.into_iter() {
        let value = match assignment.scope {
            Some(scope) => format!("{}:{}", assignment.perm_id, scope),
            None => assignment.perm_id,
        };

        map.entry(assignment.system_id).or_default().push(value);
    }

    Ok(Json(map))
}

#[rocket::get("/user/<username>/<system_id>")]
async fn user_permissions_for_system(
    username: &str,
    system_id: &str,
    db: &State<PgPool>,
) -> AppResult<Json<Vec<String>>> {
    let perms = permissions::list_all_assignments_for_user_system(username, system_id, db.inner())
        .await?
        .into_iter()
        .map(|assignment| match assignment.scope {
            Some(scope) => format!("{}:{}", assignment.perm_id, scope),
            None => assignment.perm_id,
        })
        .collect();

    Ok(Json(perms))
}

#[rocket::get("/user/<username>/<system_id>/<perm_key>")]
async fn user_has_permission(
    username: &str,
    system_id: &str,
    perm_key: PermKey<'_>,
    db: &State<PgPool>,
) -> AppResult<Json<bool>> {
    let has_permission = permissions::user_has_permission(
        username,
        system_id,
        perm_key.perm_id,
        perm_key.scope,
        db.inner(),
    )
    .await?;

    Ok(Json(has_permission))
}
