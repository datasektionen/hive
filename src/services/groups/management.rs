use std::collections::HashMap;

use crate::{
    dto::groups::EditGroupDto,
    errors::AppResult,
    guards::user::User,
    models::{ActionKind, TargetKind},
    services::{audit_log_details_for_update, audit_logs, update_if_changed},
};

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

    let old = super::details::require_one(id, domain, &mut *txn).await?;

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
            format!("{id}@{domain}"),
            &user.username,
            audit_log_details_for_update!(changed),
            &mut *txn,
        )
        .await?;

        txn.commit().await?;
    };

    Ok(())
}
