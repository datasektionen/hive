use std::collections::HashMap;

use log::*;
use serde_json::json;

use crate::{
    dto::groups::{CreateGroupDto, EditGroupDto},
    errors::{AppError, AppResult},
    guards::user::User,
    models::{ActionKind, Group, TargetKind},
    services::{audit_log_details_for_update, audit_logs, update_if_changed},
    HIVE_INTERNAL_DOMAIN,
};

pub async fn create<'v, 'x, X>(dto: &CreateGroupDto<'v>, db: X, user: &User) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    sqlx::query(
        "INSERT INTO groups (id, domain, name_sv, name_en, description_sv, description_en)
        VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(dto.id)
    .bind(dto.domain)
    .bind(dto.name_sv)
    .bind(dto.name_en)
    .bind(dto.description_sv)
    .bind(dto.description_en)
    .execute(&mut *txn)
    .await
    .map_err(|e| {
        AppError::DuplicateGroupId(dto.id.to_string(), dto.domain.to_string())
            .if_unique_violation(e)
    })?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Group,
        format!("{}@{}", *dto.id, *dto.domain),
        &user.username,
        json!({
            "new": {
                "name_sv": dto.name_sv,
                "name_en": dto.name_en,
                "description_sv": dto.description_sv,
                "description_en": dto.description_en,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn delete<'x, X>(id: &str, domain: &str, db: X, user: &User) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    if domain == HIVE_INTERNAL_DOMAIN {
        // shouldn't delete our own system-critical internal groups
        warn!("Disallowing internal group deletion from {}", user.username);
        return Err(AppError::SelfPreservation);
    }

    let mut txn = db.begin().await?;

    let old: Group = sqlx::query_as("DELETE FROM groups WHERE id = $1 AND domain = $2 RETURNING *")
        .bind(id)
        .bind(domain)
        .fetch_optional(&mut *txn)
        .await?
        .ok_or_else(|| AppError::NoSuchGroup(id.to_owned(), domain.to_owned()))?;

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::Group,
        old.key(),
        &user.username,
        json!({
            "old": {
                "name_sv": old.name_sv,
                "name_en": old.name_en,
                "description_sv": old.description_sv,
                "description_en": old.description_en,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn update<'v, 'x, X>(
    id: &str,
    domain: &str,
    dto: &EditGroupDto<'v>,
    db: X,
    user: &User,
) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let old: Group = super::details::require_one(id, domain, &mut *txn).await?;
    let key = old.key();

    let mut query = sqlx::QueryBuilder::new("UPDATE groups SET");
    let mut changed = HashMap::new();

    update_if_changed!(changed, query, name_sv, old, dto);
    update_if_changed!(changed, query, name_en, old, dto);
    update_if_changed!(changed, query, description_sv, old, dto);
    update_if_changed!(changed, query, description_en, old, dto);

    if !changed.is_empty() {
        query
            .push(" WHERE id = ")
            .push_bind(id)
            .push(" AND domain = ")
            .push_bind(domain)
            .build()
            .execute(&mut *txn)
            .await?;

        audit_logs::add_entry(
            ActionKind::Update,
            TargetKind::Group,
            key,
            &user.username,
            audit_log_details_for_update!(changed),
            &mut *txn,
        )
        .await?;

        txn.commit().await?;
    };

    Ok(())
}
