use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

use super::lang::Language;

pub struct PageContext {
    pub lang: Language,
}

#[derive(Debug)]
pub enum PageContextError {
    UnidentifiableLanguage,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PageContext {
    type Error = PageContextError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        if let Outcome::Success(lang) = req.guard::<Language>().await {
            Outcome::Success(Self { lang })
        } else {
            Outcome::Error((
                Status::InternalServerError,
                PageContextError::UnidentifiableLanguage,
            ))
        }
    }
}
