use log::*;
use rocket::{
    futures::lock::Mutex,
    request::{FromRequest, Outcome},
    Request, State,
};
use sqlx::PgPool;

use crate::{
    errors::{AppError, AppResult},
    perms::{self, HivePermission},
};

use super::{user::User, Infallible};

const HIVE_SYSTEM_ID: &str = "hive";

pub struct PermsEvaluator {
    user: Option<User>,
    db: PgPool, // cloning Pool is cheap (just an Arc)
    cache: Mutex<HivePermissionsCache>,
    // ^ Mutex is needed for internal mutability since Rocket can't give us a
    // mutable reference to PermsEvaluator (also, futures Mutex so it's Send)
}

struct HivePermissionsCache {
    perms: Vec<HivePermission>,
}

impl HivePermissionsCache {
    fn new() -> Self {
        Self { perms: Vec::new() }
    }

    fn satisfies_cached(&self, min: &HivePermission) -> Option<bool> {
        let mut found_related = false;

        for perm in &self.perms {
            if perm >= min {
                return Some(true);
            } else if perm.key() == min.key() {
                found_related = true;
            }
        }

        if found_related {
            // since other perms with the same key are cached, we can infer
            // that the user doesn't have the required permission, since
            // otherwise we would have found it somewhere in the cache
            // (assuming that all permissions with the same key are always
            // cached together at the same time)
            Some(false)
        } else {
            None
        }
    }

    fn insert(&mut self, perms: Vec<HivePermission>) {
        self.perms.extend(perms);
    }
}

impl PermsEvaluator {
    fn new(user: Option<User>, db: PgPool) -> Self {
        Self {
            user,
            db,
            cache: Mutex::new(HivePermissionsCache::new()),
        }
    }

    pub async fn satisfies(&self, min: HivePermission) -> AppResult<bool> {
        let mut cache = self.cache.lock().await;

        if let Some(cached) = cache.satisfies_cached(&min) {
            return Ok(cached);
        }

        if let Some(user) = &self.user {
            let perms = perms::get_assignments(&user.username, HIVE_SYSTEM_ID, min.key(), &self.db)
                .await?
                .into_iter()
                .map(HivePermission::try_from)
                .inspect(|r| {
                    if let Err(err) = r {
                        warn!("Got invalid Hive permission: {err:?}");
                    }
                })
                .filter_map(Result::ok)
                .collect::<Vec<_>>();

            cache.insert(perms.clone());

            for perm in perms {
                if perm >= min {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    pub async fn require(&self, min: HivePermission) -> AppResult<()> {
        if self.satisfies(min.clone()).await? {
            Ok(())
        } else {
            Err(AppError::NotAllowed(min))
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r PermsEvaluator {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        Outcome::Success(
            req.local_cache_async(async {
                let user = req.guard::<User>().await.succeeded();
                let pool = req.guard::<&State<PgPool>>().await.unwrap();

                PermsEvaluator::new(user, pool.inner().clone())
            })
            .await,
        )
    }
}
