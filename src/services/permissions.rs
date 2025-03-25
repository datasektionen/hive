use log::*;
use serde_json::json;
use uuid::Uuid;

use super::audit_logs;
use crate::{
    dto::permissions::CreatePermissionDto,
    errors::{AppError, AppResult},
    guards::{perms::PermsEvaluator, user::User},
    models::{ActionKind, AffiliatedPermissionAssignment, Permission, TargetKind},
    perms::{HivePermission, SystemsScope},
};

pub async fn list_for_system<'x, X>(system_id: &str, db: X) -> AppResult<Vec<Permission>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let permissions = sqlx::query_as(
        "SELECT *
            FROM permissions
            WHERE system_id = $1
            ORDER BY perm_id",
    )
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
    if system_id == crate::HIVE_SYSTEM_ID {
        // we manage our own permissions via database migrations
        warn!("Disallowing permissions tampering from {}", user.username);
        return Err(AppError::SelfPreservation);
    }

    let mut txn = db.begin().await?;

    let permission: Permission = sqlx::query_as(
        "INSERT INTO permissions (system_id, perm_id, has_scope, description)
        VALUES ($1, $2, $3, $4)
        RETURNING *",
    )
    .bind(system_id)
    .bind(dto.id)
    .bind(dto.scoped)
    .bind(dto.description)
    .fetch_one(&mut *txn)
    .await
    .map_err(|e| AppError::DuplicatePermissionId(dto.id.to_string()).if_unique_violation(e))?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Permission,
        permission.key(),
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

pub async fn unassign<'v, 'x, X>(
    assignment_id: Uuid,
    db: X,
    perms: &PermsEvaluator,
    user: &User,
) -> AppResult<AffiliatedPermissionAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let old: AffiliatedPermissionAssignment = sqlx::query_as(
        "DELETE FROM permission_assignments
        WHERE id = $1
        RETURNING *",
    )
    .bind(assignment_id)
    .fetch_optional(&mut *txn)
    .await?
    .ok_or_else(|| AppError::NotAllowed(HivePermission::AssignPerms(SystemsScope::Wildcard)))?;
    // ^ not a permissions problem, but prevents enumeration (we haven't checked
    // permissions yet)

    let min = HivePermission::AssignPerms(SystemsScope::Id(old.system_id.clone()));
    perms.require(min).await?;

    let details = if let Some(ref api_token_id) = old.api_token_id {
        json!({
            "old": {
                "entity_type": "api_token",
                "id": assignment_id,
                "api_token_id": api_token_id,
                "scope": old.scope,
            }
        })
    } else {
        let group_id = old.group_id.as_ref().expect("group id");
        let group_domain = old.group_domain.as_ref().expect("group domain");

        if old.system_id == crate::HIVE_SYSTEM_ID
            && group_id == crate::HIVE_ROOT_GROUP_ID
            && group_domain == crate::HIVE_INTERNAL_DOMAIN
        {
            // we manage our own root permission assignments via database migrations
            warn!(
                "Disallowing root permission assignments tampering from {}",
                user.username
            );
            return Err(AppError::SelfPreservation);
        }

        json!({
            "old": {
                "entity_type": "group",
                "id": assignment_id,
                "group_id": group_id,
                "group_domain": group_domain,
                "scope": old.scope,
            }
        })
    };

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::PermissionAssignment,
        // FIXME: consider using assignment_id as target_id
        old.key(),
        &user.username,
        details,
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(old)
}
