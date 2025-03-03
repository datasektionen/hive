use rocket::{
    http::uri::{Path, Uri},
    request::{FromRequest, Outcome},
    uri, Request,
};

use super::{user::User, Infallible};

// pub type Nav = Vec<NavLink> not allowed because of orphan rule, impl FromRequest
pub struct Nav {
    pub links: Vec<NavLink>,
}

pub struct NavLink {
    pub key: &'static str,
    pub target: &'static str,
    pub current: bool,
}

impl NavLink {
    fn new(key: &'static str, target: &'static str, here: &Path) -> Self {
        Self {
            key,
            target,
            current: target == here,
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Nav {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let mut links = Vec::new();

        let path = req.uri().path();

        if let Outcome::Success(user) = req.guard::<User>().await {
            links.push(NavLink::new("groups", "/groups", &path));

            // TODO: perms
            if true {
                links.push(NavLink::new("systems", "/systems", &path))
            }

            if true {
                links.push(NavLink::new("logs", "/logs", &path))
            }
        }

        Outcome::Success(Self { links })
    }
}
