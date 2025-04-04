use serde_json::json;
use sha2::Digest;
use uuid::Uuid;

use super::audit_logs;
use crate::{
    auth::User,
    dto::api_tokens::CreateApiTokenDto,
    errors::{AppError, AppResult},
    guards::perms::PermsEvaluator,
    models::{ActionKind, ApiToken, TargetKind},
    perms::{HivePermission, SystemsScope},
};

pub async fn list_for_system<'x, X>(system_id: &str, db: X) -> AppResult<Vec<ApiToken>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let api_tokens = sqlx::query_as(
        "SELECT at.*,
            (
                SELECT COUNT(*)
                FROM permission_assignments pa
                WHERE pa.api_token_id = at.id
            ) AS n_perms
        FROM api_tokens at
        WHERE system_id = $1
        ORDER BY expires_at, last_used_at, id",
    )
    .bind(system_id)
    .fetch_all(db)
    .await?;

    Ok(api_tokens)
}

pub struct ApiTokenCreationResult {
    pub token: ApiToken,
    pub secret: Uuid,
}

pub async fn create_new<'v, 'x, X>(
    system_id: &str,
    dto: &CreateApiTokenDto<'v>,
    db: X,
    user: &User,
) -> AppResult<ApiTokenCreationResult>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let secret = Uuid::new_v4();
    let hash = hash_secret(secret);

    let mut txn = db.begin().await?;

    let token: ApiToken = sqlx::query_as(
        "INSERT INTO api_tokens (secret, system_id, description, expires_at) VALUES ($1, $2, $3, \
         $4) RETURNING *",
    )
    .bind(hash)
    .bind(system_id)
    .bind(dto.description)
    .bind(&dto.expiration)
    .fetch_one(&mut *txn)
    .await
    .map_err(|e| AppError::AmbiguousApiToken(dto.description.to_string()).if_unique_violation(e))?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::ApiToken,
        token.id,
        &user.username,
        json!({
            "new": {
                "system_id": system_id,
                "description": dto.description,
                "expires_at": dto.expiration
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(ApiTokenCreationResult { token, secret })
}

pub async fn delete<'x, X>(
    id: &Uuid,
    db: X,
    perms: &PermsEvaluator,
    user: &User,
) -> AppResult<ApiToken>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let old: ApiToken = sqlx::query_as("DELETE FROM api_tokens WHERE id = $1 RETURNING *")
        .bind(id)
        .fetch_optional(&mut *txn)
        .await?
        .ok_or_else(|| AppError::NotAllowed(HivePermission::ManageSystems))?;
    // error is 403 instead of 404 to prevent enumeration; we haven't checked
    // any permissions yet

    perms
        .require_any_of(&[
            HivePermission::ManageSystems,
            HivePermission::ManageSystem(SystemsScope::Id(old.system_id.to_owned())),
        ])
        .await?;

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::ApiToken,
        id,
        &user.username,
        json!({
            "old": {
                "system_id": old.system_id,
                "description": old.description,
                "expires_at": old.expires_at,
                "last_used_at": old.last_used_at,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(old)
}

pub fn hash_secret(secret: Uuid) -> String {
    let hash = sha2::Sha256::new_with_prefix(secret).finalize();

    format!("{hash:x}") // hex string
}
