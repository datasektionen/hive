use rocket::{
    http::uri::Path,
    request::{FromRequest, Outcome},
    Request,
};

use super::perms::PermsEvaluator;
use crate::{errors::AppError, perms::HivePermission};

// pub type Nav = Vec<NavLink> not allowed because of orphan rule;
// need to impl FromRequest
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
    type Error = AppError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let mut links = Vec::new();

        let path = req.uri().path();

        // PermsEvaluator only exists when the user is logged in
        if let Outcome::Success(perms) = req.guard::<&PermsEvaluator>().await {
            links.push(NavLink::new("groups", "/groups", &path));

            // TODO: perms
            if true {
                links.push(NavLink::new("systems", "/systems", &path))
            }

            match perms.satisfies(HivePermission::ViewLogs).await {
                Ok(true) => links.push(NavLink::new("logs", "/logs", &path)),
                Ok(_) => {}
                Err(err) => return err.into(),
            }
        }

        Outcome::Success(Self { links })
    }
}
