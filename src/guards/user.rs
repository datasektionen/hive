use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

use super::Infallible;
use crate::auth;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for auth::User {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // TODO:
        // use https://rocket.rs/guide/v0.5/state/#request-local-state to cache
        // (ensure user is only computed once per request)
        // (maybe use Arc?)

        if let Some(user) = auth::get_current_user(req.cookies()) {
            Outcome::Success(user)
        } else {
            Outcome::Forward(Status::Unauthorized)
        }
    }
}
