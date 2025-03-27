use log::*;
use serde_json::json;

use super::audit_logs;
use crate::{
    dto::tags::CreateTagDto,
    errors::{AppError, AppResult},
    guards::user::User,
    models::{ActionKind, Tag, TargetKind},
};

pub async fn get_one<'x, X>(system_id: &str, tag_id: &str, db: X) -> AppResult<Option<Tag>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let tag = sqlx::query_as(
        "SELECT *
            FROM tags
            WHERE system_id = $1 AND tag_id = $2",
    )
    .bind(system_id)
    .bind(tag_id)
    .fetch_optional(db)
    .await?;

    Ok(tag)
}

pub async fn require_one<'x, X>(system_id: &str, tag_id: &str, db: X) -> AppResult<Tag>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    get_one(system_id, tag_id, db)
        .await?
        .ok_or_else(|| AppError::NoSuchTag(system_id.to_owned(), tag_id.to_owned()))
}

pub async fn list_for_system<'x, X>(system_id: &str, db: X) -> AppResult<Vec<Tag>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let tags = sqlx::query_as(
        "SELECT *
            FROM tags
            WHERE system_id = $1
            ORDER BY tag_id",
    )
    .bind(system_id)
    .fetch_all(db)
    .await?;

    Ok(tags)
}

pub async fn create_new<'v, 'x, X>(
    system_id: &str,
    dto: &CreateTagDto<'v>,
    db: X,
    user: &User,
) -> AppResult<Tag>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    if system_id == crate::HIVE_SYSTEM_ID {
        // we manage our own tags via database migrations
        warn!("Disallowing tags tampering from {}", user.username);
        return Err(AppError::SelfPreservation);
    }

    let mut txn = db.begin().await?;

    let tag: Tag = sqlx::query_as(
        "INSERT INTO tags
            (system_id, tag_id, supports_groups, supports_users, has_content, description)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *",
    )
    .bind(system_id)
    .bind(dto.id)
    .bind(dto.supports_groups)
    .bind(dto.supports_users)
    .bind(dto.has_content)
    .bind(dto.description)
    .fetch_one(&mut *txn)
    .await
    .map_err(|e| AppError::DuplicateTagId(dto.id.to_string()).if_unique_violation(e))?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Tag,
        tag.key(),
        &user.username,
        json!({
            "new": {
                "supports_groups": dto.supports_groups,
                "supports_users": dto.supports_users,
                "has_content": dto.has_content,
                "description": dto.description,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(tag)
}
