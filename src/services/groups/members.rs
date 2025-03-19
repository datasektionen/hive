use chrono::Local;

use crate::{
    errors::AppResult,
    models::{GroupMember, Subgroup},
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
            '1970-01-01'::DATE AS \"from\", -- FIXME: actual date
            '1970-01-01'::DATE AS \"until\" -- FIXME: actual date
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
