use chrono::Local;
use log::*;
use serde_json::json;
use sha2::Digest;
use uuid::Uuid;

use super::{audit_logs, pg_args};
use crate::{
    dto::permissions::{
        AssignPermissionToApiTokenDto, AssignPermissionToGroupDto, CreatePermissionDto,
    },
    errors::{AppError, AppResult},
    guards::{lang::Language, perms::PermsEvaluator, user::User},
    models::{
        ActionKind, AffiliatedPermissionAssignment, BasePermissionAssignment, Permission,
        TargetKind,
    },
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

pub async fn list_all_assignments_for_user<'x, X>(
    username: &str,
    db: X,
) -> AppResult<Vec<BasePermissionAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let today = Local::now().date_naive();

    let assignments = sqlx::query_as(
        "SELECT pa.system_id, pa.perm_id, pa.scope
        FROM permission_assignments pa
        JOIN all_groups_of($1, $2) ag
            ON ag.id = pa.group_id
            AND ag.domain = pa.group_domain
        ORDER BY pa.system_id, pa.perm_id, pa.scope",
    )
    .bind(username)
    .bind(today)
    .fetch_all(db)
    .await?;

    Ok(assignments)
}

pub async fn list_all_assignments_for_user_system<'x, X>(
    username: &str,
    system_id: &str,
    db: X,
) -> AppResult<Vec<BasePermissionAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let today = Local::now().date_naive();

    let assignments = sqlx::query_as(
        "SELECT pa.system_id, pa.perm_id, pa.scope
        FROM permission_assignments pa
        JOIN all_groups_of($1, $2) ag
            ON ag.id = pa.group_id
            AND ag.domain = pa.group_domain
        WHERE pa.system_id = $3
        ORDER BY pa.perm_id, pa.scope",
    )
    .bind(username)
    .bind(today)
    .bind(system_id)
    .fetch_all(db)
    .await?;

    Ok(assignments)
}

pub async fn list_all_assignments_for_token_system<'x, X>(
    secret: Uuid,
    system_id: &str,
    db: X,
) -> AppResult<Vec<BasePermissionAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let hash = sha2::Sha256::new_with_prefix(secret).finalize();
    let hash = format!("{hash:x}"); // hex string

    let assignments = sqlx::query_as(
        "SELECT pa.system_id, pa.perm_id, pa.scope
        FROM permission_assignments pa
        JOIN api_tokens at
            ON at.id = pa.api_token_id
        WHERE at.secret = $1
            AND pa.system_id = $2
        ORDER BY pa.perm_id, pa.scope",
    )
    .bind(hash)
    .bind(system_id)
    .fetch_all(db)
    .await?;

    Ok(assignments)
}

pub async fn user_has_permission<'x, X>(
    username: &str,
    system_id: &str,
    perm_id: &str,
    scope: Option<&str>,
    db: X,
) -> AppResult<bool>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let today = Local::now().date_naive();

    let authorized = sqlx::query_scalar(
        "SELECT COUNT(pa.*) > 0
        FROM permission_assignments pa
        JOIN all_groups_of($1, $2) ag
            ON ag.id = pa.group_id
            AND ag.domain = pa.group_domain
        WHERE pa.system_id = $3
            AND pa.perm_id = $4
            AND (
                pa.scope IS NOT DISTINCT FROM $5
                OR pa.scope = '*'
            )",
    )
    .bind(username)
    .bind(today)
    .bind(system_id)
    .bind(perm_id)
    .bind(scope)
    .fetch_one(db)
    .await?;

    Ok(authorized)
}

pub async fn token_has_permission<'x, X>(
    secret: Uuid,
    system_id: &str,
    perm_id: &str,
    scope: Option<&str>,
    db: X,
) -> AppResult<bool>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let hash = sha2::Sha256::new_with_prefix(secret).finalize();
    let hash = format!("{hash:x}"); // hex string

    let authorized = sqlx::query_scalar(
        "SELECT COUNT(pa.*) > 0
        FROM permission_assignments pa
        JOIN api_tokens at
            ON at.id = pa.api_token_id
        WHERE at.secret = $1
            AND pa.system_id = $2
            AND pa.perm_id = $3
            AND (
                pa.scope IS NOT DISTINCT FROM $4
                OR pa.scope = '*'
            )",
    )
    .bind(hash)
    .bind(system_id)
    .bind(perm_id)
    .bind(scope)
    .fetch_one(db)
    .await?;

    Ok(authorized)
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
    query.push(" AND pa.group_id IS NOT NULL AND pa.group_domain IS NOT NULL");

    let mut assignments: Vec<AffiliatedPermissionAssignment> =
        query.build_query_as().fetch_all(db).await?;

    for assignment in &mut assignments {
        let min = HivePermission::AssignPerms(SystemsScope::Id(assignment.system_id.clone()));
        // query should be OK since perms are cached by perm_id
        assignment.can_manage = Some(perms.satisfies(min).await?);
    }

    Ok(assignments)
}

pub async fn list_api_token_assignments<'x, X>(
    system_id: &str,
    perm_id: &str,
    label_lang: Option<&Language>,
    db: X,
    perms: &PermsEvaluator,
) -> AppResult<Vec<AffiliatedPermissionAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let mut query = sqlx::QueryBuilder::new(
        "SELECT pa.*,
            at.system_id AS api_token_system_id",
    );

    if label_lang.is_some() {
        query.push(", at.description AS label");
    }

    query.push(
        " FROM permission_assignments pa
        JOIN api_tokens at
            ON at.id = pa.api_token_id
        WHERE pa.system_id = ",
    );

    query.push_bind(system_id);
    query.push(" AND pa.perm_id = ");
    query.push_bind(perm_id);
    query.push(" AND pa.api_token_id IS NOT NULL");

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

pub async fn delete<'x, X>(system_id: &str, perm_id: &str, db: X, user: &User) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    if system_id == crate::HIVE_SYSTEM_ID {
        // we manage our own permissions via database migrations
        warn!("Disallowing permissions tampering from {}", user.username);
        return Err(AppError::SelfPreservation);
    }

    let mut txn = db.begin().await?;

    let old: Permission = sqlx::query_as(
        "DELETE FROM permissions
        WHERE system_id = $1
            AND perm_id = $2
        RETURNING *",
    )
    .bind(system_id)
    .bind(perm_id)
    .fetch_optional(&mut *txn)
    .await?
    .ok_or_else(|| AppError::NoSuchPermission(system_id.to_owned(), perm_id.to_owned()))?;

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::Permission,
        old.key(),
        &user.username,
        json!({
            "old": {
                "has_scope": old.has_scope,
                "description": old.description,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn assign_to_group<'v, 'x, X>(
    system_id: &str,
    perm_id: &str,
    dto: &AssignPermissionToGroupDto<'v>,
    label_lang: Option<&Language>,
    db: X,
    user: &User,
) -> AppResult<AffiliatedPermissionAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let has_scope = has_scope(system_id, perm_id, &mut *txn).await?;

    if has_scope && dto.scope.is_none() {
        return Err(AppError::MissingPermissionScope(
            system_id.to_string(),
            perm_id.to_string(),
        ));
    } else if !has_scope && dto.scope.is_some() {
        return Err(AppError::ExtraneousPermissionScope(
            system_id.to_string(),
            perm_id.to_string(),
        ));
    }

    let mut query = sqlx::QueryBuilder::with_arguments(
        "INSERT INTO permission_assignments (system_id, perm_id, scope, group_id, group_domain)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *, TRUE AS can_manage",
        pg_args!(
            system_id,
            perm_id,
            dto.scope,
            dto.group.id,
            dto.group.domain
        ),
    );

    if let Some(lang) = label_lang {
        query.push(", (SELECT ");
        match lang {
            Language::Swedish => query.push("name_sv"),
            Language::English => query.push("name_en"),
        };
        query.push(
            " FROM groups gs
            WHERE gs.id = $4
                AND gs.domain = $5
            ) AS label",
        );
    }

    let mut assignment: AffiliatedPermissionAssignment = query
        .build_query_as()
        .fetch_one(&mut *txn)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(err) if err.is_unique_violation() => {
                AppError::DuplicatePermissionAssignment(
                    system_id.to_string(),
                    perm_id.to_string(),
                    dto.scope.as_deref().map(ToString::to_string),
                )
            }
            sqlx::Error::Database(err) if err.is_foreign_key_violation() => {
                AppError::NoSuchGroup(dto.group.id.to_string(), dto.group.domain.to_string())
            }
            _ => e.into(),
        })?;

    assignment.can_manage = Some(true);

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::PermissionAssignment,
        assignment.key(),
        &user.username,
        json!({
            "new": {
                "entity_type": "group",
                "id": assignment.id,
                "group_id": assignment.group_id,
                "group_domain": assignment.group_domain,
                "scope": assignment.scope,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(assignment)
}

pub async fn assign_to_api_token<'v, 'x, X>(
    system_id: &str,
    perm_id: &str,
    dto: &AssignPermissionToApiTokenDto<'v>,
    label_lang: Option<&Language>,
    db: X,
    user: &User,
) -> AppResult<AffiliatedPermissionAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let has_scope = has_scope(system_id, perm_id, &mut *txn).await?;

    if has_scope && dto.scope.is_none() {
        return Err(AppError::MissingPermissionScope(
            system_id.to_string(),
            perm_id.to_string(),
        ));
    } else if !has_scope && dto.scope.is_some() {
        return Err(AppError::ExtraneousPermissionScope(
            system_id.to_string(),
            perm_id.to_string(),
        ));
    }

    let mut query = sqlx::QueryBuilder::with_arguments(
        "INSERT INTO permission_assignments (system_id, perm_id, scope, api_token_id)
        VALUES ($1, $2, $3, $4)
        RETURNING *, TRUE AS can_manage",
        pg_args!(system_id, perm_id, dto.scope, dto.token),
    );

    if label_lang.is_some() {
        // there doesn't seem an easy way to re-use the same subquery and return
        // 2 values from there into 2 separate columns...
        query.push(
            ", (
                SELECT system_id
                FROM api_tokens at
                WHERE at.id = $4
            ) AS api_token_system_id, (
                SELECT description
                FROM api_tokens at
                WHERE at.id = $4
            ) AS label",
        );
    }

    let mut assignment: AffiliatedPermissionAssignment = query
        .build_query_as()
        .fetch_one(&mut *txn)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(err) if err.is_unique_violation() => {
                AppError::DuplicatePermissionAssignment(
                    system_id.to_string(),
                    perm_id.to_string(),
                    dto.scope.as_deref().map(ToString::to_string),
                )
            }
            sqlx::Error::Database(err) if err.is_foreign_key_violation() => {
                AppError::NoSuchApiToken(dto.token)
            }
            _ => e.into(),
        })?;

    assignment.can_manage = Some(true);

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::PermissionAssignment,
        assignment.key(),
        &user.username,
        json!({
            "new": {
                "entity_type": "api_token",
                "id": assignment.id,
                "api_token_id": assignment.api_token_id,
                "scope": assignment.scope,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(assignment)
}

pub async fn unassign<'x, X>(
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
