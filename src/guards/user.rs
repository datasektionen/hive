use rocket::{
    request::{FromRequest, Outcome},
    Request,
};

pub struct User {
    pub username: String,
    pub display_name: String,
}

#[derive(Debug)]
pub enum UserIdentificationError {
    Something,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = UserIdentificationError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // TODO: properly, plus:
        // use https://rocket.rs/guide/v0.5/state/#request-local-state to cache
        // (ensure user is only computed once per request)
        Outcome::Success(Self {
            username: "dummy".to_owned(),
            display_name: "John Doe".to_owned(),
        })
    }
}
