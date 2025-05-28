use chrono::Local;
use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request, State,
};
use sqlx::{prelude::FromRow, PgPool};
use uuid::Uuid;

use super::token::BearerToken;
use crate::{
    api::HiveApiPermission,
    errors::{AppError, AppResult},
    perms::HivePermission,
    services::api_tokens,
};

const IMPERSONATION_HEADER: &str = "X-Hive-Impersonate-System";

#[derive(FromRow)]
pub struct ApiConsumer {
    pub api_token_id: Uuid,
    pub system_id: String,
}

impl ApiConsumer {
    pub async fn satisfies<'x, X>(&self, min: HiveApiPermission, db: X) -> AppResult<bool>
    where
        X: sqlx::Executor<'x, Database = sqlx::Postgres>,
    {
        // (ignores scope since all current permissions don't have any scope)
        let satisfies = sqlx::query_scalar(
            "SELECT COUNT(*) > 0
            FROM permission_assignments
            WHERE api_token_id = $1
                AND perm_id = $2
                AND system_id = $3",
        )
        .bind(self.api_token_id)
        .bind(HivePermission::from(min).key())
        .bind(crate::HIVE_SYSTEM_ID)
        .fetch_one(db)
        .await?;

        Ok(satisfies)
    }

    pub async fn require<'x, X>(&self, min: HiveApiPermission, db: X) -> AppResult<()>
    where
        X: sqlx::Executor<'x, Database = sqlx::Postgres>,
    {
        if self.satisfies(min.clone(), db).await? {
            Ok(())
        } else {
            Err(AppError::NotAllowed(min.into()))
        }
    }

    pub async fn try_impersonate<'x, X>(
        self,
        other_system_id: &str,
        db: X,
    ) -> AppResult<Option<Self>>
    where
        X: sqlx::Executor<'x, Database = sqlx::Postgres>,
    {
        // easier to redo the query here than to adapt `satisfies` to accept
        // scope; might be worth reconsidering if more scoped perms are made
        let satisfies = sqlx::query_scalar(
            "SELECT COUNT(*) > 0
            FROM permission_assignments
            WHERE perm_id = 'api-impersonate-system'
                AND api_token_id = $1
                AND system_id = $2
                AND (scope = $3 OR scope = '*')",
        )
        .bind(self.api_token_id)
        .bind(crate::HIVE_SYSTEM_ID)
        .bind(other_system_id)
        .fetch_one(db)
        .await?;

        let consumer = if satisfies {
            Some(Self {
                api_token_id: self.api_token_id,
                system_id: other_system_id.to_owned(),
            })
        } else {
            None
        };

        Ok(consumer)
    }
}

#[derive(Debug)]
pub enum InvalidApiConsumer {
    MissingBearerToken,
    MalformedUuid,
    UnknownApiToken,
    UnauthorizedImpersonation,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiConsumer {
    type Error = InvalidApiConsumer;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        if let Some(bearer) = req.guard::<BearerToken>().await.succeeded() {
            if let Ok(secret) = Uuid::try_parse(bearer.0) {
                let hash = api_tokens::hash_secret(secret);
                let now = Local::now();

                let pool = req.guard::<&State<PgPool>>().await.unwrap();

                let result: Result<ApiConsumer, _> = sqlx::query_as(
                    "UPDATE api_tokens
                    SET last_used_at = $1
                    WHERE secret = $2
                        AND (expires_at IS NULL OR expires_at >= $1)
                    RETURNING id AS api_token_id, system_id",
                )
                .bind(now)
                .bind(hash)
                .fetch_one(pool.inner())
                .await;

                if let Ok(consumer) = result {
                    if let Some(other_system_id) = req.headers().get_one(IMPERSONATION_HEADER) {
                        if let Ok(Some(impersonated)) = consumer
                            .try_impersonate(other_system_id, pool.inner())
                            .await
                        {
                            Outcome::Success(impersonated)
                        } else {
                            Outcome::Error((
                                Status::Forbidden,
                                InvalidApiConsumer::UnauthorizedImpersonation,
                            ))
                        }
                    } else {
                        Outcome::Success(consumer)
                    }
                } else {
                    Outcome::Error((Status::Unauthorized, InvalidApiConsumer::UnknownApiToken))
                }
            } else {
                Outcome::Error((Status::Unauthorized, InvalidApiConsumer::MalformedUuid))
            }
        } else {
            Outcome::Error((Status::Unauthorized, InvalidApiConsumer::MissingBearerToken))
        }
    }
}
