use crate::{
    errors::AppResult,
    models::{ActionKind, TargetKind},
};

pub async fn add_entry<'a, 'q, X, P>(
    action_kind: ActionKind,
    target_kind: TargetKind,
    target_id: P, // &str, uuid, etc.
    actor_username: &'q str,
    details: serde_json::Value,
    db: X,
) -> AppResult<()>
where
    X: sqlx::Executor<'a, Database = sqlx::Postgres>,
    P: 'q + sqlx::Encode<'q, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>,
{
    sqlx::query(
        "INSERT INTO audit_logs (action_kind, target_kind, target_id, actor, details) VALUES \
         ($1, $2, $3, $4, $5)",
    )
    .bind(action_kind)
    .bind(target_kind)
    .bind(target_id)
    .bind(actor_username)
    .bind(details)
    .execute(db)
    .await?;

    Ok(())
}
