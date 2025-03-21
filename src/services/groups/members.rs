use chrono::Local;
use serde_json::json;

use crate::{
    dto::groups::{AddMemberDto, AddSubgroupDto},
    errors::{AppError, AppResult},
    guards::user::User,
    models::{ActionKind, GroupMember, Subgroup, TargetKind},
    services::audit_logs,
};

pub async fn get_direct_members<'x, X>(id: &str, domain: &str, db: X) -> AppResult<Vec<GroupMember>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let today = Local::now().date_naive();

    let members = sqlx::query_as(
        "SELECT *
        FROM direct_memberships
        WHERE group_id = $1
        AND group_domain = $2
        AND $3 BETWEEN \"from\" AND \"until\"
        ORDER BY manager DESC, username, id", // DESC makes true come first
    )
    .bind(id)
    .bind(domain)
    .bind(today)
    .fetch_all(db)
    .await?;

    Ok(members)
}

pub async fn get_all_members<'x, X>(id: &str, domain: &str, db: X) -> AppResult<Vec<GroupMember>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let today = Local::now().date_naive();

    let members = sqlx::query_as(
        "SELECT username,
            bool_or(manager) AS manager,
            min(\"from\") AS \"from\",
            max(\"until\") AS \"until\"
        FROM all_members_of($1, $2, $3)
        GROUP BY username
        ORDER BY manager DESC, username", // DESC makes true come first
    )
    .bind(id)
    .bind(domain)
    .bind(today)
    .fetch_all(db)
    .await?;

    Ok(members)
}

pub async fn get_direct_subgroups<'x, X>(id: &str, domain: &str, db: X) -> AppResult<Vec<Subgroup>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let subgroups = sqlx::query_as(
        "SELECT gs.*, sg.manager
        FROM subgroups sg
        JOIN groups gs
            ON gs.id = sg.child_id
            AND gs.domain = sg.child_domain
        WHERE sg.parent_id = $1
        AND sg.parent_domain = $2
        ORDER BY sg.manager DESC, gs.id, gs.domain", // DESC makes true come first
    )
    .bind(id)
    .bind(domain)
    .fetch_all(db)
    .await?;

    Ok(subgroups)
}

pub async fn add_subgroup<'v, 'x, X>(
    parent_id: &str,
    parent_domain: &str,
    dto: &AddSubgroupDto<'v>,
    db: X,
    user: &User,
) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    if parent_id == dto.child.id && parent_domain == dto.child.domain {
        // can't be a subgroup of itself
        return Err(AppError::InvalidSubgroup(
            parent_id.to_owned(),
            parent_domain.to_owned(),
        ));
    }

    let mut txn = db.begin().await?;

    let loop_detected = sqlx::query_scalar(
        "SELECT COUNT(*) > 0
        FROM all_subgroups_of($1, $2)
        WHERE child_id = $3
            AND child_domain = $4",
    )
    .bind(dto.child.id)
    .bind(dto.child.domain)
    .bind(parent_id)
    .bind(parent_domain)
    .fetch_one(&mut *txn)
    .await?;

    if loop_detected {
        return Err(AppError::InvalidSubgroup(
            dto.child.id.to_owned(),
            dto.child.domain.to_owned(),
        ));
    }

    sqlx::query(
        "INSERT INTO subgroups (parent_id, parent_domain, child_id, child_domain, manager)
        VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(parent_id)
    .bind(parent_domain)
    .bind(dto.child.id)
    .bind(dto.child.domain)
    .bind(dto.manager)
    .execute(&mut *txn)
    .await
    .map_err(|e| {
        AppError::DuplicateSubgroup(dto.child.id.to_string(), dto.child.domain.to_string())
            .if_unique_violation(e)
    })?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Membership,
        format!("{}@{}", parent_id, parent_domain),
        &user.username,
        json!({
            "new": {
                "member_type": "subgroup",
                "child_id": dto.child.id,
                "child_domain": dto.child.domain,
                "manager": dto.manager,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn add_member<'v, 'x, X>(
    id: &str,
    domain: &str,
    dto: &AddMemberDto<'v>,
    db: X,
    user: &User,
) -> AppResult<GroupMember>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let redundant = sqlx::query_scalar(
        "SELECT COUNT(*) > 0
        FROM direct_memberships
        WHERE username = $1
            AND group_id = $2
            AND group_domain = $3
            AND \"from\" <= $4
            AND \"until\" >= $5
            AND manager >= $6",
    )
    .bind(dto.username)
    .bind(id)
    .bind(domain)
    .bind(&dto.from)
    .bind(&dto.until)
    .bind(dto.manager)
    .fetch_one(&mut *txn)
    .await?;

    if redundant {
        return Err(AppError::RedundantMembership(dto.username.to_string()));
    }

    let added = sqlx::query_as(
        "INSERT INTO direct_memberships(username, group_id, group_domain, \"from\", \"until\", \
         manager)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *",
    )
    .bind(dto.username)
    .bind(id)
    .bind(domain)
    .bind(&dto.from)
    .bind(&dto.until)
    .bind(dto.manager)
    .fetch_one(&mut *txn)
    .await?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Membership,
        format!("{}@{}", id, domain),
        &user.username,
        json!({
            "new": {
                "member_type": "member",
                "username": dto.username,
                "from": dto.from,
                "until": dto.until,
                "manager": dto.manager,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(added)
}
