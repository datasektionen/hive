use std::collections::{HashMap, HashSet};

use log::*;
use rocket::{
    futures::lock::Mutex,
    request::{FromRequest, Outcome},
    Request, State,
};
use sqlx::PgPool;

use super::{user::User, Infallible};
use crate::{
    errors::{AppError, AppResult},
    perms::{self, HivePermission},
};

const HIVE_SYSTEM_ID: &str = "hive";

pub struct PermsEvaluator {
    user: Option<User>,
    db: PgPool, // cloning Pool is cheap (just an Arc)
    cache: Mutex<HivePermissionsCache>,
    // ^ Mutex is needed for internal mutability since Rocket can't give us a
    // mutable reference to PermsEvaluator (also, futures Mutex so it's Send)
}

struct HivePermissionsCache {
    entries: HashMap<&'static str, HashSet<HivePermission>>,
    // ^ empty Vec if we need to cache a user having *no* permissions for a key
}

impl HivePermissionsCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn satisfies_cached(&self, min: &HivePermission) -> Option<bool> {
        if let Some(perms) = self.entries.get(min.key()) {
            for perm in perms {
                if perm >= min {
                    return Some(true);
                }
            }
        } else {
            return None;
        }

        // assumes that all permissions with the same key (i.e., different
        // scopes) are always cached together -- meaning that we can infer
        // "not-allowed" if the key is in the cache and no matching perm
        // was found
        Some(false)
    }

    fn insert<I: IntoIterator<Item = HivePermission>>(&mut self, key: &'static str, perms: I) {
        let set = HashSet::from_iter(perms);

        if self.entries.insert(key, set).is_some() {
            warn!("Overwriting permissions cache key {key}; this should not be possible!");
        }
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

            cache.insert(min.key(), perms.clone());

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

    pub async fn require_any_of(&self, possibilities: &[HivePermission]) -> AppResult<()> {
        for min in possibilities {
            if self.satisfies(min.clone()).await? {
                return Ok(());
            }
        }

        Err(AppError::NotAllowed(
            possibilities
                .last()
                .expect("Empty possible permissions array")
                .clone(),
        ))
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
