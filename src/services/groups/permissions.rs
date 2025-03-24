use crate::{errors::AppResult, models::PermissionAssignment};

pub async fn get_all_assignments<'x, X>(
    id: &str,
    domain: &str,
    db: X,
) -> AppResult<Vec<PermissionAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let assignments = sqlx::query_as(
        "SELECT pa.*, ps.description
        FROM permission_assignments pa
        JOIN permissions ps
            ON pa.system_id = ps.system_id
            AND pa.perm_id = ps.perm_id
        WHERE pa.group_id = $1
            AND pa.group_domain = $2
        ORDER BY system_id, perm_id, scope",
    )
    .bind(id)
    .bind(domain)
    .fetch_all(db)
    .await?;

    Ok(assignments)
}
