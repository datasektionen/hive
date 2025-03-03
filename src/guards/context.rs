use std::{borrow::Cow, fmt};

use rocket::{
    request::{FromRequest, Outcome},
    Request,
};

use super::{lang::Language, nav::Nav, user::User, Infallible};

pub struct PageContext {
    pub lang: Language,
    pub user: Option<User>,
    pub nav: Nav,
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

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PageContext {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let lang = req.guard::<Language>().await.unwrap();
        let user = req.guard::<User>().await.succeeded();
        let nav = req.guard::<Nav>().await.unwrap();

        Outcome::Success(Self { lang, user, nav })
    }
}
