use chrono::Local;
use log::*;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

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
        AND until >= $3
        ORDER BY (\"from\" <= $3) DESC, manager DESC, username, id", // DESC makes true come first
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

pub async fn get_membership_group<'x, X>(
    membership_id: &Uuid,
    db: X,
) -> AppResult<Option<(String, String)>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let row = sqlx::query(
        "SELECT group_id, group_domain
        FROM direct_memberships
        WHERE id = $1",
    )
    .bind(membership_id)
    .fetch_optional(db)
    .await?;

    if let Some(row) = row {
        let id = row.try_get("group_id")?;
        let domain = row.try_get("group_domain")?;

        Ok(Some((id, domain)))
    } else {
        Ok(None)
    }
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
    .map_err(|e| match e {
        sqlx::Error::Database(err) if err.is_unique_violation() => {
            AppError::DuplicateSubgroup(dto.child.id.to_string(), dto.child.domain.to_string())
        }
        sqlx::Error::Database(err) if err.is_foreign_key_violation() => {
            AppError::NoSuchGroup(dto.child.id.to_string(), dto.child.domain.to_string())
        }
        _ => e.into(),
    })?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Membership,
        format!("{}@{}", parent_id, parent_domain),
        user.username(),
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

pub async fn remove_subgroup<'x, X>(
    parent_id: &str,
    parent_domain: &str,
    child_id: &str,
    child_domain: &str,
    db: X,
    user: &User,
) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let manager = sqlx::query_scalar(
        "DELETE FROM subgroups
        WHERE parent_id = $1
            AND parent_domain = $2
            AND child_id = $3
            AND child_domain = $4
        RETURNING manager",
    )
    .bind(parent_id)
    .bind(parent_domain)
    .bind(child_id)
    .bind(child_domain)
    .fetch_optional(&mut *txn)
    .await?;

    let manager: bool = if let Some(manager) = manager {
        manager
    } else {
        // child was not a (direct) subgroup of parent, so there's nothing to do
        // (just return without committing the transaction)
        return Ok(());
    };

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::Membership,
        format!("{}@{}", parent_id, parent_domain),
        user.username(),
        json!({
            "old": {
                "member_type": "subgroup",
                "child_id": child_id,
                "child_domain": child_domain,
                "manager": manager,
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

    let added: GroupMember = sqlx::query_as(
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
        // FIXME: consider using added.id as target_id
        format!("{}@{}", id, domain),
        user.username(),
        json!({
            "new": {
                "member_type": "member",
                "id": added.id.as_ref().unwrap(),
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

// membership_id is enough, but group id/domain is good just to double-check
pub async fn remove_member<'x, X>(
    membership_id: &Uuid,
    group_id: &str,
    group_domain: &str,
    db: X,
    user: &User,
) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let today = Local::now().date_naive();

    let mut txn = db.begin().await?;

    let member = sqlx::query_as(
        "DELETE FROM direct_memberships
        WHERE id = $1
            AND group_id = $2
            AND group_domain = $3
        RETURNING *",
    )
    .bind(membership_id)
    .bind(group_id)
    .bind(group_domain)
    .fetch_optional(&mut *txn)
    .await?;

    let member: GroupMember = if let Some(member) = member {
        member
    } else {
        // ID was not associated with this group, so there's nothing to do
        // (just return without committing the transaction)
        return Ok(());
    };

    // ideally we would do this here instead of a separate query in the route
    // handler, but it doesn't work because &mut *txn is not Copy as required
    // let group = GroupRef::from_row(&row)?;
    // super::details::require_authority(...)

    let last_root_member =
        sqlx::query_scalar("SELECT COUNT(*) = 0 FROM all_members_of($1, $2, $3)")
            .bind(crate::HIVE_ROOT_GROUP_ID)
            .bind(crate::HIVE_INTERNAL_DOMAIN)
            .bind(today)
            .fetch_one(&mut *txn)
            .await?;

    if last_root_member {
        // cannot remove our last administrator
        // (note that this sadly doesn't prevent that their membership naturally
        // expires and we end up with no administrators anyway)
        warn!(
            "Disallowing last administrator removal from {}",
            user.username()
        );
        return Err(AppError::SelfPreservation);
    };

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::Membership,
        // FIXME: consider using membership_id as target_id
        format!("{}@{}", group_id, group_domain),
        user.username(),
        json!({
            "old": {
                "member_type": "member",
                "id": membership_id,
                "username": member.username,
                "from": member.from,
                "until": member.until,
                "manager": member.manager,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn conditional_bootstrap<'x, X>(username: &str, db: X) -> AppResult<bool>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    // add user to root group iff it currently has no members

    let today = Local::now().date_naive();

    let mut txn = db.begin().await?;

    // not using `all_members_of` because we're fine with empty subgroups, so as
    // to support the case where someone (non-root) has permissions to manage
    // one of root's subgroups, in which case a bootstrap should not be
    // triggered
    let has_members = sqlx::query_scalar(
        "SELECT
            EXISTS (
                SELECT 1
                FROM direct_memberships
                WHERE
                    group_id = $1
                    AND group_domain = $2
                    AND $3 BETWEEN \"from\" AND until
            )
            OR
            EXISTS (
                SELECT 1
                FROM subgroups
                WHERE
                    parent_id = $1
                    AND parent_domain = $2
            )
        AS has_members",
    )
    .bind(crate::HIVE_ROOT_GROUP_ID)
    .bind(crate::HIVE_INTERNAL_DOMAIN)
    .bind(today)
    .fetch_one(&mut *txn)
    .await?;

    if has_members {
        // nothing to do
        return Ok(false);
    }

    let expiration = today
        .checked_add_months(chrono::Months::new(12 * 1000))
        .expect(
            r#"Bootstrapping is not supported after Friday, December 31st, 261143.

            However, Hive has already been in use for 259118 years at this point,
            so you might want to consider migrating to a newer system..."#,
        );

    sqlx::query(
        "INSERT INTO direct_memberships
        (username, group_id, group_domain, \"from\", until, manager)
        VALUES ($1, $2, $3, $4, $5, true)",
    )
    .bind(username)
    .bind(crate::HIVE_ROOT_GROUP_ID)
    .bind(crate::HIVE_INTERNAL_DOMAIN)
    .bind(today)
    .bind(expiration)
    .execute(&mut *txn)
    .await?;

    warn!("Bootstrapped user {username} as Hive root until {expiration}");

    txn.commit().await?;

    Ok(true)
}
