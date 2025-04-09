use serde_json::json;

use crate::{
    dto::tags::AssignTagDto,
    errors::{AppError, AppResult},
    guards::{perms::PermsEvaluator, user::User},
    models::{ActionKind, Tag, TagAssignment, TargetKind},
    perms::{HivePermission, SystemsScope},
    services::{audit_logs, tags},
};

pub async fn get_all_assignments<'x, X>(
    id: &str,
    domain: &str,
    db: X,
    perms: &PermsEvaluator,
) -> AppResult<Vec<TagAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let mut assignments: Vec<TagAssignment> = sqlx::query_as(
        "SELECT ta.*, ts.description
        FROM tag_assignments ta
        JOIN tags ts
            ON ta.system_id = ts.system_id
            AND ta.tag_id = ts.tag_id
        WHERE ta.group_id = $1
            AND ta.group_domain = $2
        ORDER BY system_id, tag_id, content",
    )
    .bind(id)
    .bind(domain)
    .fetch_all(db)
    .await?;

    for assignment in &mut assignments {
        let min = HivePermission::AssignTags(SystemsScope::Id(assignment.system_id.clone()));
        // query should be OK since perms are cached by perm_id
        assignment.can_manage = Some(perms.satisfies(min).await?);
    }

    Ok(assignments)
}

pub async fn get_all_assignable<'x, X>(perms: &PermsEvaluator, db: X) -> AppResult<Vec<Tag>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let systems_filter = get_systems_filter(perms).await?;

    let mut query = sqlx::QueryBuilder::new(
        "SELECT *
        FROM tags
        WHERE supports_groups",
    );

    if let Some(system_ids) = systems_filter {
        if system_ids.is_empty() {
            return Ok(vec![]);
        }

        query.push(" AND system_id = ANY(");
        query.push_bind(system_ids);
        query.push(")");
    }

    let permissions = query.build_query_as().fetch_all(db).await?;

    Ok(permissions)
}

async fn get_systems_filter(perms: &PermsEvaluator) -> AppResult<Option<Vec<String>>> {
    let hive_perms = perms
        .fetch_all_related(HivePermission::AssignTags(SystemsScope::Any))
        .await?;

    let mut systems_filter = vec![];
    for perm in hive_perms {
        if let HivePermission::AssignTags(scope) = perm {
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
    dto: &AssignTagDto<'_>,
    db: X,
    user: &User,
) -> AppResult<TagAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let has_content = tags::has_content(dto.tag.system_id, dto.tag.tag_id, &mut *txn).await?;

    if has_content && dto.content.is_none() {
        return Err(AppError::MissingTagContent(
            dto.tag.system_id.to_string(),
            dto.tag.tag_id.to_string(),
        ));
    } else if !has_content && dto.content.is_some() {
        return Err(AppError::ExtraneousTagContent(
            dto.tag.system_id.to_string(),
            dto.tag.tag_id.to_string(),
        ));
    }

    let assignment: TagAssignment = sqlx::query_as(
        "INSERT INTO tag_assignments (system_id, tag_id, content, group_id, group_domain)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING
            *,
            (
                SELECT description
                FROM tags
                WHERE system_id = $1
                    AND tag_id = $2
            ) AS description,
            TRUE AS can_manage",
    )
    .bind(dto.tag.system_id)
    .bind(dto.tag.tag_id)
    .bind(dto.content)
    .bind(group_id)
    .bind(group_domain)
    .fetch_one(&mut *txn)
    .await
    .map_err(|e| {
        AppError::DuplicateTagAssignment(
            dto.tag.system_id.to_string(),
            dto.tag.tag_id.to_string(),
            dto.content.as_deref().map(ToString::to_string),
        )
        .if_unique_violation(e)
    })?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::TagAssignment,
        assignment.key(),
        user.username(),
        json!({
            "new": {
                "entity_type": "group",
                "id": assignment.id,
                "group_id": group_id,
                "group_domain": group_domain,
                "content": assignment.content,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(assignment)
}
