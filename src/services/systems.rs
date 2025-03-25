use log::*;
use rocket::futures::TryStreamExt;
use serde_json::json;

use super::audit_logs;
use crate::{
    dto::systems::{CreateSystemDto, EditSystemDto},
    errors::{AppError, AppResult},
    guards::{perms::PermsEvaluator, user::User},
    models::{ActionKind, System, TargetKind},
    perms::{HivePermission, SystemsScope},
    sanitizers::SearchTerm,
};

pub async fn ensure_exists<'x, X>(id: &str, db: X) -> AppResult<()>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    sqlx::query("SELECT id FROM systems WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;

    Ok(())
}

pub async fn get_one<'x, X>(id: &str, db: X) -> AppResult<Option<System>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let system = sqlx::query_as("SELECT * FROM systems WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?;

    Ok(system)
}

pub async fn list_manageable<'x, X>(
    q: Option<&str>,
    fully_authorized: bool,
    db: X,
    perms: &PermsEvaluator,
) -> AppResult<Vec<System>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let mut query = sqlx::QueryBuilder::new("SELECT * FROM systems");

    if let Some(search) = q {
        // this will push the same bind twice even though both could be
        // references to the same $1 param... there doesn't seem to be a
        // way to avoid this, since push_bind adds $n to the query itself
        let term = SearchTerm::from(search).anywhere();
        query.push(" WHERE id ILIKE ");
        query.push_bind(term.clone());
        query.push(" OR description ILIKE ");
        query.push_bind(term);
    }

    query.push(" ORDER BY id");

    let mut result = query.build_query_as::<System>().fetch(db);

    if fully_authorized {
        Ok(result.try_collect().await?)
    } else {
        let mut systems = vec![];

        while let Some(system) = result.try_next().await? {
            let scope = SystemsScope::Id(system.id.clone());

            if perms.satisfies(HivePermission::ManageSystem(scope)).await? {
                systems.push(system);
            }
        }

        Ok(systems)
    }
}

pub async fn create_new<'v, 'x, X>(dto: &CreateSystemDto<'v>, db: X, user: &User) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    sqlx::query("INSERT INTO systems (id, description) VALUES ($1, $2)")
        .bind(dto.id)
        .bind(dto.description)
        .execute(&mut *txn)
        .await
        .map_err(|e| AppError::DuplicateSystemId(dto.id.to_string()).if_unique_violation(e))?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::System,
        dto.id,
        &user.username,
        json!({"new": {"description": dto.description}}),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn delete<'x, X>(id: &str, db: X, user: &User) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    if id == crate::HIVE_SYSTEM_ID {
        // shouldn't delete ourselves
        warn!("Disallowing self-deletion from {}", user.username);
        return Err(AppError::SelfPreservation);
    }

    let mut txn = db.begin().await?;

    let old: System = sqlx::query_as("DELETE FROM systems WHERE id = $1 RETURNING *")
        .bind(id)
        .fetch_optional(&mut *txn)
        .await?
        .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::System,
        id,
        &user.username,
        json!({"old": {"description": old.description}}),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn update<'v, 'x, X>(
    id: &str,
    dto: &EditSystemDto<'v>,
    db: X,
    user: &User,
) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    // subquery runs before update
    let old_description: String = sqlx::query_scalar(
        "UPDATE systems SET description = $1 WHERE id = $2 RETURNING (SELECT description FROM \
         systems WHERE id = $2)",
    )
    .bind(dto.description)
    .bind(id)
    .fetch_optional(&mut *txn)
    .await?
    .ok_or_else(|| AppError::NoSuchSystem(id.to_owned()))?;

    if *dto.description != old_description {
        audit_logs::add_entry(
            ActionKind::Update,
            TargetKind::System,
            id,
            &user.username,
            json!({
                "old": {"description": old_description},
                "new": {"description": dto.description},
            }),
            &mut *txn,
        )
        .await?;

        txn.commit().await?;
    }

    Ok(())
}
