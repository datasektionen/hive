use std::collections::{HashMap, HashSet};

use log::*;
use serde::Deserialize;

use crate::errors::{AppError, AppResult};

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const USER_AGENT: &str = "hive-identity-resolver";

pub struct IdentityResolver {
    endpoint: String,
    client: reqwest::Client,
}

impl IdentityResolver {
    pub fn new(endpoint: Option<String>) -> Option<Self> {
        if let Some(endpoint) = endpoint {
            let client = reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(USER_AGENT)
                .build()
                .expect("failed to build resolver reqwest client");

            Some(Self { endpoint, client })
        } else {
            None
        }
    }

    pub async fn resolve_usernames<'s>(
        &self,
        usernames: impl Iterator<Item = &'s str>,
    ) -> AppResult<HashMap<String, String>> {
        let params: HashSet<_> = usernames.map(|u| ("u", u)).collect();
        // ^ HashSet means deduplication, we only need to ask each username once

        let entries: HashMap<String, ResolvedEntry> = self
            .client
            .get(&self.endpoint)
            .query(&[("format", "map")])
            .query(&params)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(AppError::IdentityResolutionError)?
            .json()
            .await
            .map_err(AppError::IdentityResolutionError)?;

        trace!("Identity resolution returned: {:?}", &entries);

        let display_names = entries
            .into_iter()
            .map(|(k, v)| (k, v.display_name()))
            .collect();

        Ok(display_names)
    }

    pub async fn resolve_one(&self, username: &str) -> AppResult<Option<String>> {
        let result = self
            .client
            .get(&self.endpoint)
            .query(&[("format", "single"), ("u", username)])
            .send()
            .await;

        if let Ok(ref response) = result {
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                // resolver does not know this username
                return Ok(None);
            }
        }

        let name = result
            .and_then(reqwest::Response::error_for_status)
            .map_err(AppError::IdentityResolutionError)?
            .json::<ResolvedEntry>()
            .await
            .map_err(AppError::IdentityResolutionError)?
            .display_name();

        Ok(Some(name))
    }

    pub async fn populate_identities<T>(
        &self,
        items: &mut [T],
        username_getter: impl Fn(&T) -> &str,
        display_name_setter: impl Fn(&mut T, String),
    ) -> AppResult<()> {
        let usernames = items.iter().map(&username_getter);
        let result = self.resolve_usernames(usernames).await?;

        for item in items {
            let username = username_getter(item);
            if let Some(display_name) = result.get(username) {
                display_name_setter(item, display_name.clone());
            }
        }

        Ok(())
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ResolvedEntry {
    first_name: String,
    family_name: String,
}

impl ResolvedEntry {
    fn display_name(&self) -> String {
        format!("{} {}", self.first_name, self.family_name)
    }
}
