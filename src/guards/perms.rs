use std::collections::{HashMap, HashSet};

use log::*;
use rocket::{
    futures::lock::{Mutex, MutexGuard},
    http::Status,
    request::{FromRequest, Outcome},
    Request, State,
};
use sqlx::PgPool;

use super::user::User;
use crate::{
    errors::{AppError, AppResult},
    perms::{self, HivePermission},
};

const HIVE_SYSTEM_ID: &str = "hive";

pub struct PermsEvaluator {
    user: User,
    db: PgPool, // cloning Pool is cheap (just an Arc)
    cache: Mutex<HivePermissionsCache>,
    // ^ Mutex is needed for internal mutability since Rocket can't give us a
    // mutable reference to PermsEvaluator (also, futures Mutex so it's Send)
}

struct HivePermissionsCache {
    entries: HashMap<&'static str, HashSet<HivePermission>>,
    // ^ empty Set if we need to cache a user having *no* permissions for a key
}

impl HivePermissionsCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn fetch_all_with_key<'a>(&'a self, key: &str) -> Option<&'a HashSet<HivePermission>> {
        self.entries.get(key)
    }

    fn satisfies_cached(&self, min: &HivePermission) -> Option<bool> {
        if let Some(perms) = self.fetch_all_with_key(min.key()) {
            for perm in perms {
                if perm >= min {
                    return Some(true);
                }
            }

            // assumes that all permissions with the same key (i.e., different
            // scopes) are always cached together -- meaning that we can infer
            // "not-allowed" if the key is in the cache and no matching perm
            // was found
            Some(false)
        } else {
            None
        }
    }

    fn insert<I: IntoIterator<Item = HivePermission>>(&mut self, key: &'static str, perms: I) {
        let set = HashSet::from_iter(perms);

        if self.entries.insert(key, set).is_some() {
            warn!("Overwriting permissions cache key {key}; this should not be possible!");
        }
    }
}

impl PermsEvaluator {
    fn new(user: User, db: PgPool) -> Self {
        Self {
            user,
            db,
            cache: Mutex::new(HivePermissionsCache::new()),
        }
    }

    async fn load_into_cache(
        &self,
        cache: &mut MutexGuard<'_, HivePermissionsCache>,
        key: &'static str,
    ) -> AppResult<Vec<HivePermission>> {
        let username = &self.user.username;
        let perms = perms::get_assignments(username, HIVE_SYSTEM_ID, key, &self.db)
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

        cache.insert(key, perms.clone());

        Ok(perms)
    }

    // better type-checking integrity to take in a HivePermission instead of
    // a key &str directly in public functions
    pub async fn fetch_all_related(&self, probe: HivePermission) -> AppResult<Vec<HivePermission>> {
        let mut cache = self.cache.lock().await;

        if let Some(cached) = cache.fetch_all_with_key(probe.key()) {
            Ok(cached.iter().cloned().collect())
        } else {
            Ok(self.load_into_cache(&mut cache, probe.key()).await?)
        }
    }

    pub async fn satisfies(&self, min: HivePermission) -> AppResult<bool> {
        let mut cache = self.cache.lock().await;

        // we don't use fetch_all_related because this returns faster
        // for cached permissions that come first
        if let Some(cached) = cache.satisfies_cached(&min) {
            return Ok(cached);
        }

        for perm in self.load_into_cache(&mut cache, min.key()).await? {
            if perm >= min {
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub async fn satisfies_any_of(&self, possibilities: &[HivePermission]) -> AppResult<bool> {
        for min in possibilities {
            if self.satisfies(min.clone()).await? {
                return Ok(true);
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
        if self.satisfies_any_of(possibilities).await? {
            Ok(())
        } else {
            Err(AppError::NotAllowed(
                possibilities
                    .last()
                    .expect("Empty possible permissions array")
                    .clone(),
            ))
        }
    }
}

#[derive(Debug)]
pub struct UserNotAuthenticatedError;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r PermsEvaluator {
    type Error = UserNotAuthenticatedError;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let result = req
            .local_cache_async(async {
                if let Outcome::Success(user) = req.guard::<User>().await {
                    let pool = req.guard::<&State<PgPool>>().await.unwrap();

                    Some(PermsEvaluator::new(user, pool.inner().clone()))
                } else {
                    None
                }
            })
            .await;

        if let Some(perms) = result {
            Outcome::Success(perms)
        } else {
            Outcome::Error((Status::Unauthorized, UserNotAuthenticatedError))
        }
    }
}
