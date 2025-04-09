use serde_json::json;

use crate::{
    dto::permissions::AssignPermissionDto,
    errors::{AppError, AppResult},
    guards::{perms::PermsEvaluator, user::User},
    models::{ActionKind, Permission, PermissionAssignment, TargetKind},
    perms::{HivePermission, SystemsScope},
    services::{audit_logs, permissions},
};

pub async fn get_all_assignments<'x, X>(
    id: &str,
    domain: &str,
    db: X,
    perms: &PermsEvaluator,
) -> AppResult<Vec<PermissionAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let mut assignments: Vec<PermissionAssignment> = sqlx::query_as(
        "SELECT pa.*, ps.description
        FROM permission_assignments pa
        JOIN permissions ps
            ON pa.system_id = ps.system_id
            AND pa.perm_id = ps.perm_id
        WHERE pa.group_id = $1
            AND pa.group_domain = $2
        ORDER BY system_id, perm_id, scope",
    )
    .bind(id)
    .bind(domain)
    .fetch_all(db)
    .await?;

    for assignment in &mut assignments {
        let min = HivePermission::AssignPerms(SystemsScope::Id(assignment.system_id.clone()));
        // query should be OK since perms are cached by perm_id
        assignment.can_manage = Some(perms.satisfies(min).await?);
    }

    Ok(assignments)
}

pub async fn get_all_assignable<'x, X>(perms: &PermsEvaluator, db: X) -> AppResult<Vec<Permission>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let systems_filter = get_systems_filter(perms).await?;

    let mut query = sqlx::QueryBuilder::new("SELECT * FROM permissions");

    if let Some(system_ids) = systems_filter {
        if system_ids.is_empty() {
            return Ok(vec![]);
        }

        query.push(" WHERE system_id = ANY(");
        query.push_bind(system_ids);
        query.push(")");
    }

    let permissions = query.build_query_as().fetch_all(db).await?;

    Ok(permissions)
}

async fn get_systems_filter(perms: &PermsEvaluator) -> AppResult<Option<Vec<String>>> {
    let hive_perms = perms
        .fetch_all_related(HivePermission::AssignPerms(SystemsScope::Any))
        .await?;

    let mut systems_filter = vec![];
    for perm in hive_perms {
        if let HivePermission::AssignPerms(scope) = perm {
            match scope {
                SystemsScope::Wildcard => return Ok(None),
                SystemsScope::Id(id) => systems_filter.push(id),
                SystemsScope::Any => unreachable!("? is not a real scope"),
            }
        }
    }

    Ok(Some(systems_filter))
}

pub async fn assign<'x, X>(
    group_id: &str,
    group_domain: &str,
    dto: &AssignPermissionDto<'_>,
    db: X,
    user: &User,
) -> AppResult<PermissionAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let has_scope = permissions::has_scope(dto.perm.system_id, dto.perm.perm_id, &mut *txn).await?;

    if has_scope && dto.scope.is_none() {
        return Err(AppError::MissingPermissionScope(
            dto.perm.system_id.to_string(),
            dto.perm.perm_id.to_string(),
        ));
    } else if !has_scope && dto.scope.is_some() {
        return Err(AppError::ExtraneousPermissionScope(
            dto.perm.system_id.to_string(),
            dto.perm.perm_id.to_string(),
        ));
    }

    let assignment: PermissionAssignment = sqlx::query_as(
        "INSERT INTO permission_assignments (system_id, perm_id, scope, group_id, group_domain)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING
            *,
            (
                SELECT description
                FROM permissions
                WHERE system_id = $1
                    AND perm_id = $2
            ) AS description,
            TRUE AS can_manage",
    )
    .bind(dto.perm.system_id)
    .bind(dto.perm.perm_id)
    .bind(dto.scope)
    .bind(group_id)
    .bind(group_domain)
    .fetch_one(&mut *txn)
    .await
    .map_err(|e| {
        AppError::DuplicatePermissionAssignment(
            dto.perm.system_id.to_string(),
            dto.perm.perm_id.to_string(),
            dto.scope.as_deref().map(ToString::to_string),
        )
        .if_unique_violation(e)
    })?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::PermissionAssignment,
        assignment.key(),
        user.username(),
        json!({
            "new": {
                "entity_type": "group",
                "id": assignment.id,
                "group_id": group_id,
                "group_domain": group_domain,
                "scope": dto.scope,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(assignment)
}
