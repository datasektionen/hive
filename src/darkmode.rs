use std::sync::Mutex;

use log::*;

use crate::errors::{AppError, AppResult};

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const USER_AGENT: &str = "hive-darkmode-client";

pub struct DarkmodeClient {
    endpoint: String,
    client: reqwest::Client,
    state: Mutex<bool>,
}

impl DarkmodeClient {
    pub async fn new(endpoint: Option<String>) -> AppResult<Option<Self>> {
        if let Some(endpoint) = endpoint {
            let client = reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(USER_AGENT)
                .build()
                .expect("failed to build darkmode reqwest client");

            // Get current darkmode state
            let response = client.get(&endpoint).send().await;

            let data = match response {
                Ok(response) => response.json().await,
                Err(e) => return Err(AppError::DarkmodeError(e)),
            };

            let state = match data {
                Ok(state) => state,
                Err(e) => return Err(AppError::DarkmodeError(e)),
            };

            Ok(Some(Self {
                endpoint,
                client,
                state,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn get_state(&self) -> bool {
        let guard = self.state.lock().expect("poisoned");

        *guard
    }

    pub async fn update_state(&self) -> AppResult<()> {
        let state = self
            .client
            .get(&self.endpoint)
            .send()
            .await
            .map_err(AppError::DarkmodeError)?
            .json()
            .await
            .map_err(AppError::DarkmodeError)?;

        let mut guard = self.state.lock().expect("poisoned");

        info!("Setting darkmode to {state}");

        *guard = state;

        Ok(())
    }
}
