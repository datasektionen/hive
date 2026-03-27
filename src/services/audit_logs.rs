use crate::{
    dto::logs::LogsFilterDto,
    errors::AppResult,
    models::{ActionKind, AuditLog, TargetKind},
};

pub async fn add_entry<'a, 'q, X>(
    action_kind: ActionKind,
    target_kind: TargetKind,
    target_id: impl ToString, // &str, uuid, etc.
    actor_username: &'q str,
    details: serde_json::Value,
    db: X,
) -> AppResult<()>
where
    X: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    sqlx::query(
        "INSERT INTO audit_logs (action_kind, target_kind, target_id, actor, details) VALUES ($1, \
         $2, $3, $4, $5)",
    )
    .bind(action_kind)
    .bind(target_kind)
    .bind(target_id.to_string())
    .bind(actor_username)
    .bind(details)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn get_logs_paged<'a, X>(
    db: X,
    filter: &LogsFilterDto<'_>,
    offset: u32,
    limit: u32,
) -> AppResult<Vec<AuditLog>>
where
    X: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    let mut query = sqlx::QueryBuilder::new(
        "SELECT action_kind,
            target_kind,
            target_id,
            actor,
            details,
            stamp
        FROM audit_logs",
    );

    filter.apply(&mut query);

    query
        .push(" OFFSET ")
        .push_bind(i32::try_from(offset).unwrap_or(0));
    query
        .push(" LIMIT ")
        .push_bind(i32::try_from(limit).unwrap_or(50));

    let logs = query.build_query_as().fetch_all(db).await?;

    Ok(logs)
}

pub async fn list_actors<'a, X>(db: X) -> AppResult<Vec<String>>
where
    X: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    let actors: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT actor
        FROM audit_logs
        ORDER BY actor",
    )
    .fetch_all(db)
    .await?;

    Ok(actors.into_iter().map(|actor| actor.0).collect())
}

pub async fn list_target_ids<'a, X>(db: X) -> AppResult<Vec<String>>
where
    X: sqlx::Executor<'a, Database = sqlx::Postgres>,
{
    let ids: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT target_id
        FROM audit_logs
        ORDER BY target_id",
    )
    .fetch_all(db)
    .await?;

    Ok(ids.into_iter().map(|id| id.0).collect())
}
