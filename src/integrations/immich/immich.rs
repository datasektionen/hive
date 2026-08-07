use std::fmt;

use chrono::{DateTime, Local};
use log::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const USER_AGENT: &str = "hive-immich-integration";

pub struct ImmichAPIClient {
    reqwest_client: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl ImmichAPIClient {
    pub fn new(base_url: String, api_key: String) -> Result<Self, &'static str> {
        let reqwest_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                error!("Organisation API failed to build reqwest client: {e}");

                "Failed to build Reqwest client"
            })?;

        Ok(Self {
            reqwest_client,
            base_url,
            api_key,
        })
    }

    async fn exec_request<R: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        url: impl reqwest::IntoUrl + Copy + fmt::Display,
        body: Option<impl Serialize + fmt::Debug>,
        error_message: &'static str,
    ) -> Result<Option<R>, &'static str> {
        let request = self
            .reqwest_client
            .request(method.clone(), url)
            .header("x-api-key", self.api_key.clone());

        let request = if let Some(ref body) = body {
            request.json(&body)
        } else {
            request
        };

        let response = request
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                error!("Organisation API failed to execute request ({url}): {e:?}");
                error!("Sent body: {body:?}");

                error_message
            })?;

        // We don't care about the response body except when we do a GET
        if method != reqwest::Method::GET {
            return Ok(None);
        }

        let decoded = response.json().await.map_err(|e| {
            error!("Organisation API failed to decode response JSON ({url}): {e:?}");

            "Failed to decode response JSON"
        })?;

        Ok(Some(decoded))
    }

    pub async fn list_albums(&self) -> Result<Vec<AlbumResponseDto>, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!("{}/api/albums", self.base_url),
            None::<()>,
            "Failed to list albums",
        )
        .await
        .and_then(|op| op.ok_or("Failed to list albums"))
    }

    pub async fn share_album(
        &self,
        album_id: &str,
        users: AddUsersToAlbumDto,
    ) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::PUT,
            &format!("{}/api/albums/{album_id}/users", self.base_url),
            Some(users),
            "Failed to share album with user",
        )
        .await
    }

    pub async fn remove_user(
        &self,
        album_id: &str,
        user_id: &str,
    ) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::DELETE,
            &format!("{}/api/albums/{album_id}/user/{user_id}", self.base_url,),
            None::<()>,
            "Failed to remove user from album",
        )
        .await
    }

    pub async fn list_users(&self) -> Result<Vec<UserResponseDto>, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!("{}/api/users", self.base_url),
            None::<()>,
            "Failed to list users",
        )
        .await
        .and_then(|op| op.ok_or("Failed to list users"))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumResponseDto {
    pub id: String,
    pub album_name: String,
    pub album_users: Vec<AlbumUserResponseDto>,
    pub start_date: DateTime<Local>,
    pub end_date: DateTime<Local>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AlbumUserRole {
    Editor,
    Owner,
    Viewer,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AlbumUserResponseDto {
    pub role: AlbumUserRole,
    pub user: UserResponseDto,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UserResponseDto {
    pub email: String,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumUserAddDto {
    pub role: AlbumUserRole,
    pub user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUsersToAlbumDto {
    pub album_users: Vec<AlbumUserAddDto>
}
