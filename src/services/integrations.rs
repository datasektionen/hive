use serde_json::json;

use crate::{
    errors::AppResult,
    models::{ActionKind, TagAssignment, TargetKind},
    services::audit_logs,
};

pub async fn get_self_service<'x, X>(
    integration_id: &str,
    tag_id: &str,
    username: &str,
    db: X,
) -> AppResult<Option<String>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let value = sqlx::query_scalar(
        "SELECT content
        FROM tag_assignments
        WHERE system_id = $1
            AND tag_id = $2
            AND username = $3
        ORDER BY id
        LIMIT 1",
    )
    .bind(integration_id)
    .bind(tag_id)
    .bind(username)
    .fetch_optional(db)
    .await?;

    Ok(value)
}

pub async fn set_self_service<'x, X>(
    integration_id: &str,
    tag_id: &str,
    username: &str,
    value: &str,
    db: X,
) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    sqlx::query(
        "DELETE
        FROM tag_assignments
        WHERE system_id = $1
            AND tag_id = $2
            AND username = $3",
    )
    .bind(integration_id)
    .bind(tag_id)
    .bind(username)
    .execute(&mut *txn)
    .await?;

    let assignment: TagAssignment = sqlx::query_as(
        "INSERT INTO tag_assignments
            (system_id, tag_id, username, content)
        VALUES ($1, $2, $3, $4)
        RETURNING *, '[unused]' AS description",
    )
    .bind(integration_id)
    .bind(tag_id)
    .bind(username)
    .bind(value)
    .fetch_one(&mut *txn)
    .await?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::TagAssignment,
        assignment.key(),
        username,
        json!({
            "new": {
                "entity_type": "user",
                "id": assignment.id,
                "username": username,
                "content": assignment.content,
            },
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}
