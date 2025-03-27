use log::*;
use serde_json::json;
use uuid::Uuid;

use super::audit_logs;
use crate::{
    dto::permissions::CreatePermissionDto,
    errors::{AppError, AppResult},
    guards::{lang::Language, perms::PermsEvaluator, user::User},
    models::{ActionKind, AffiliatedPermissionAssignment, Permission, TargetKind},
    perms::{HivePermission, SystemsScope},
};

pub async fn get_one<'x, X>(system_id: &str, perm_id: &str, db: X) -> AppResult<Option<Permission>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let permission = sqlx::query_as(
        "SELECT *
            FROM permissions
            WHERE system_id = $1 AND perm_id = $2",
    )
    .bind(system_id)
    .bind(perm_id)
    .fetch_optional(db)
    .await?;

    Ok(permission)
}

pub async fn require_one<'x, X>(system_id: &str, perm_id: &str, db: X) -> AppResult<Permission>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    get_one(system_id, perm_id, db)
        .await?
        .ok_or_else(|| AppError::NoSuchPermission(system_id.to_owned(), perm_id.to_owned()))
}

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

pub async fn list_group_assignments<'x, X>(
    system_id: &str,
    perm_id: &str,
    label_lang: Option<&Language>,
    db: X,
    perms: &PermsEvaluator,
) -> AppResult<Vec<AffiliatedPermissionAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let mut query = sqlx::QueryBuilder::new("SELECT pa.*");

    match label_lang {
        Some(Language::Swedish) => {
            query.push(", gs.name_sv AS label");
        }
        Some(Language::English) => {
            query.push(", gs.name_en AS label");
        }
        None => {}
    }

    query.push(" FROM permission_assignments pa");

    if label_lang.is_some() {
        query.push(
            " JOIN groups gs
                ON gs.id = pa.group_id
                AND gs.domain = pa.group_domain",
        );
    }

    query.push(" WHERE pa.system_id = ");
    query.push_bind(system_id);
    query.push(" AND pa.perm_id = ");
    query.push_bind(perm_id);
    query.push(" AND group_id IS NOT NULL AND group_domain IS NOT NULL");

    let mut assignments: Vec<AffiliatedPermissionAssignment> =
        query.build_query_as().fetch_all(db).await?;

    for assignment in &mut assignments {
        let min = HivePermission::AssignPerms(SystemsScope::Id(assignment.system_id.clone()));
        // query should be OK since perms are cached by perm_id
        assignment.can_manage = Some(perms.satisfies(min).await?);
    }

    Ok(assignments)
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

pub async fn has_scope<'x, X>(system_id: &str, perm_id: &str, db: X) -> AppResult<bool>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    sqlx::query_scalar(
        "SELECT has_scope
        FROM permissions
        WHERE system_id = $1
            AND perm_id = $2",
    )
    .bind(system_id)
    .bind(perm_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NoSuchPermission(system_id.to_string(), perm_id.to_string()))
}
