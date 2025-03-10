use log::*;
use serde_json::json;

use crate::{
    dto::permissions::CreatePermissionDto,
    errors::{AppError, AppResult},
    guards::user::User,
    models::{ActionKind, Permission, TargetKind},
};

use super::audit_logs;

pub async fn list_for_system<'x, X>(system_id: &str, db: X) -> AppResult<Vec<Permission>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let permissions =
        sqlx::query_as("SELECT * FROM permissions WHERE system_id = $1 ORDER BY perm_id")
            .bind(system_id)
            .fetch_all(db)
            .await?;

    Ok(permissions)
}

pub async fn create_new<'v, 'x, X>(
    system_id: &str,
    dto: &CreatePermissionDto<'v>,
    db: X,
    user: &User,
) -> AppResult<Permission>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    if system_id == "hive" {
        // we manage our own permissions via database migrations
        warn!("Disallowing permissions tampering from {}", user.username);
        return Err(AppError::SelfPreservation);
    }

    let mut txn = db.begin().await?;

    let permission: Permission = sqlx::query_as(
        "INSERT INTO permissions (system_id, perm_id, has_scope, description) VALUES ($1, $2, \
         $3, $4) RETURNING *",
    )
    .bind(system_id)
    .bind(dto.id)
    .bind(dto.scoped)
    .bind(dto.description)
    .fetch_one(&mut *txn)
    .await?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Permission,
        permission.full_id(),
        &user.username,
        json!({
            "new": {
                "has_scope": dto.scoped,
                "description": dto.description,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(permission)
}
