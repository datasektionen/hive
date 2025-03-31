use chrono::Local;
use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request, State,
};
use sqlx::{prelude::FromRow, PgPool};
use uuid::Uuid;

use crate::services::api_tokens;

use super::token::BearerToken;

#[derive(FromRow)]
pub struct ApiConsumer {
    pub api_token_id: Uuid,
    pub system_id: String,
}

#[derive(Debug)]
pub enum InvalidApiConsumer {
    MissingBearerToken,
    MalformedUuid,
    UnknownApiToken,
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

                let result = sqlx::query_as(
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
                    Outcome::Success(consumer)
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
