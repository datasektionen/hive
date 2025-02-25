use std::{borrow::Cow, fmt};

use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

use super::{lang::Language, user::User};

pub struct PageContext {
    pub lang: Language,
    pub user: Option<User>,
}

// Convenience aliases to prevent having to ctx.lang.t
impl PageContext {
    pub fn t<'a>(&self, key: &'a str) -> Cow<'a, str> {
        self.lang.t(key)
    }

    pub fn t1<'a, T: fmt::Display>(&self, key: &'a str, x: T) -> Cow<'a, str> {
        self.lang.t1(key, x)
    }
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
            if let Outcome::Success(user) = req.guard::<User>().await {
                Outcome::Success(Self {
                    lang,
                    user: Some(user),
                })
            } else {
                todo!("no user")
            }
        } else {
            Outcome::Error((
                Status::InternalServerError,
                PageContextError::UnidentifiableLanguage,
            ))
        }
    }
}
