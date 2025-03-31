use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

pub struct BearerToken<'t>(pub &'t str);

#[derive(Debug)]
pub enum MissingBearerToken {
    Header,
    AuthScheme,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for BearerToken<'r> {
    type Error = MissingBearerToken;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        if let Some(value) = req.headers().get_one("Authorization") {
            if let Some(token) = value.strip_prefix("Bearer ") {
                Outcome::Success(Self(token))
            } else {
                Outcome::Error((Status::Unauthorized, MissingBearerToken::AuthScheme))
            }
        } else {
            Outcome::Error((Status::Unauthorized, MissingBearerToken::Header))
        }
    }
}
