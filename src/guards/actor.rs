use std::borrow::Cow;

use rocket::{
    http::Status,
    request::Request,
    request::{FromRequest, Outcome},
};

use crate::guards::{Infallible, api::consumer::ApiConsumer, user::User};

#[derive(Clone)]
pub enum Actor {
    User(User),
    Api(ApiConsumer),
}

impl Actor {
    pub fn key(&self) -> Cow<'_, str> {
        match self {
            Self::User(user) => Cow::Borrowed(user.username()),
            Self::Api(api) => Cow::Owned(api.api_token_id.to_string()),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Actor {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let result = req
            .local_cache_async(async {
                if let Outcome::Success(user) = req.guard::<User>().await {
                    Some(Actor::User(user))
                } else if let Outcome::Success(api) = req.guard::<ApiConsumer>().await {
                    Some(Actor::Api(api))
                } else {
                    None
                }
            })
            .await;

        match result {
            Some(actor) => Outcome::Success(actor.clone()),
            None => Outcome::Forward(Status::Unauthorized),
        }
    }
}
