use serde_json::json;
use uuid::Uuid;
use std::collections::HashMap;

use crate::{
    errors::AppResult,
    integrations::MANIFESTS,
    models::{ActionKind, IntegrationTaskLogEntry, IntegrationTaskRun, System, TagAssignment, TargetKind},
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

pub async fn list_integrations<'x, X>(db: X) -> AppResult<Vec<System>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let integration_ids: Vec<String> = MANIFESTS
        .iter()
        .map(|integration| integration.id.to_owned())
        .collect();

    let integrations = sqlx::query_as(
        "SELECT name, description
        FROM systems
        WHERE system_id IN (SELECT UNNEST($1::TEXT[]))
        ORDER BY id",
    )
    .bind(integration_ids)
    .fetch_all(db)
    .await?;

    Ok(integrations)
}

pub async fn list_settings<'x, X>(
    integration_id: &str,
    db: X,
) -> AppResult<HashMap<String, serde_json::Value>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let settings: HashMap<String, serde_json::Value> = sqlx::query_as(
        "SELECT setting_id, setting_value
        FROM integration_settings
        WHERE integration_id = $1
        ORDER BY setting_id",
    )
    .bind(integration_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect();

    Ok(settings)
}

pub async fn list_runs<'x, X>(
    integration_id: &str,
    db: X,
) -> AppResult<Vec<IntegrationTaskRun>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let runs  = sqlx::query_as(
        "SELECT
            run_id,
            task_id,
            start_stamp,
            end_stamp,
            succeeded
        FROM integration_task_runs
        WHERE integration_id = $1
        ORDER BY start_stamp ASC",
    )
    .bind(integration_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect();

    Ok(runs)
}

pub async fn list_logs<'x, X>(
    run_id: Uuid,
    db: X,
) -> AppResult<Vec<IntegrationTaskLogEntry>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let logs = sqlx::query_as(
        "SELECT stamp, kind, message
        FROM integration_task_logs
        WHERE run_id = $1
        ORDER BY stamp ASC",
    )
    .bind(run_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect();

    Ok(logs)
}
