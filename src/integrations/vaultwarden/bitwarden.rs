use std::{collections::HashMap, fmt};

use log::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const USER_AGENT: &str = "hive-vaultwarden-integration";

pub struct VaultManagementAPIClient {
    reqwest_client: reqwest::Client,
    base_url: String,
    org_id: String,
    master_password: String,
}

impl VaultManagementAPIClient {
    pub fn new(
        base_url: String,
        org_id: String,
        master_password: String,
    ) -> Result<Self, &'static str> {
        let reqwest_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                error!("Vault Management API failed to build reqwest client: {e}");

                "Failed to build Reqwest client"
            })?;

        Ok(Self {
            reqwest_client,
            base_url,
            org_id,
            master_password,
        })
    }

    async fn exec_request(
        &self,
        method: reqwest::Method,
        url: impl reqwest::IntoUrl + Copy + fmt::Display,
        body: Option<impl Serialize + fmt::Debug>,
        error_message: &'static str,
    ) -> Result<(), &'static str> {
        let request = self.reqwest_client.request(method.clone(), url);

        let request = if let Some(ref body) = body {
            request.json(&body)
        } else {
            request
        };

        request
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                error!("Vault Management API failed to execute request ({url}): {e:?}");
                error!("Sent body: {body:?}");

                error_message
            })?;

        // We don't care about the response, as long as it is OK 200
        Ok(())
    }

    pub async fn unlock(&self) -> Result<(), &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            &format!("{}/unlock", self.base_url),
            Some(Unlock {
                password: self.master_password.clone(),
            }),
            "Failed to unlock vault",
        )
        .await
    }

    pub async fn lock(&self) -> Result<(), &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            &format!("{}/lock", self.base_url),
            None::<()>,
            "Failed to lock vault",
        )
        .await
    }

    pub async fn confirm(&self, id: &str) -> Result<(), &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            &format!(
                "{}/confirm/org-member/{id}?organizationId={}",
                self.base_url, self.org_id
            ),
            None::<()>,
            "Failed to lock vault",
        )
        .await
    }
}

// The access token only lives for 2 hours but as long as the client is only used for one task run
// it should be fine
pub struct OrganisationAPIClient {
    reqwest_client: reqwest::Client,
    base_url: String,
    org_id: String,
    access_token: String,
}

impl OrganisationAPIClient {
    pub async fn new(
        base_url: String,
        org_id: String,
        client_id: &str,
        client_secret: &str,
    ) -> Result<Self, &'static str> {
        let reqwest_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                error!("Organisation API failed to build reqwest client: {e}");

                "Failed to build Reqwest client"
            })?;

        let access_token =
            Self::get_access_token(&reqwest_client, &base_url, client_id, client_secret)
                .await?
                .access_token;

        Ok(Self {
            reqwest_client,
            base_url,
            org_id,
            access_token,
        })
    }

    async fn get_access_token(
        client: &reqwest::Client,
        base_url: &str,
        client_id: &str,
        client_secret: &str,
    ) -> Result<AccessTokenResponse, &'static str> {
        let mut params = HashMap::new();
        params.insert("grant_type", "client_credentials");
        params.insert("scope", "api");
        // ^ Need to use a user token because vaultwarden does not support organization api keys fully
        params.insert("client_id", client_id);
        params.insert("client_secret", client_secret);
        params.insert("device_identifier", USER_AGENT);
        params.insert("device_name", USER_AGENT);
        params.insert("device_type", "2"); // server

        let response = client
            .post(format!("{base_url}/identity/connect/token"))
            .form(&params)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                error!("Organisation API failed to get access token: {e}");

                "Failed to get access token"
            })?
            .json()
            .await
            .map_err(|e| {
                error!("Organisation API failed to decode access token JSON: {e}");

                "Failed to decode access token JSON"
            })?;

        Ok(response)
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
            .bearer_auth(&self.access_token);

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

    pub async fn list_groups(&self) -> Result<GroupList, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!(
                "{}/api/organizations/{}/groups/details",
                self.base_url, self.org_id
            ),
            None::<()>,
            "Failed to list groups",
        )
        .await
        .and_then(|op| op.ok_or("Failed to list groups"))
    }

    pub async fn list_org_members(&self) -> Result<UserList, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!(
                "{}/api/organizations/{}/users?includeGroups=true",
                self.base_url, self.org_id
            ),
            None::<()>,
            "Failed to list users",
        )
        .await
        .and_then(|op| op.ok_or("Failed to list users"))
    }

    pub async fn delete_group(&self, id: &str) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::DELETE,
            &format!(
                "{}/api/organizations/{}/groups/{id}/delete",
                self.base_url, self.org_id
            ),
            None::<()>,
            "Failed to delete group",
        )
        .await
    }

    pub async fn create_group(&self, body: GroupSync) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            &format!("{}/api/organizations/{}/groups", self.base_url, self.org_id),
            Some(body),
            "Failed to create group",
        )
        .await
    }

    pub async fn update_group(
        &self,
        id: &str,
        body: GroupSync,
    ) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::PUT,
            &format!(
                "{}/api/organizations/{}/groups/{id}",
                self.base_url, self.org_id
            ),
            Some(body),
            "Failed to sync group",
        )
        .await
    }

    pub async fn invite_users(&self, body: InviteData) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            &format!(
                "{}/api/organizations/{}/users/invite",
                self.base_url, self.org_id
            ),
            Some(body),
            "Failed to invite users",
        )
        .await
    }
}

#[derive(Debug, Serialize)]
struct Unlock {
    password: String,
}

#[derive(Debug, Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    expires_in: u32,
    token_type: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupList {
    pub continuation_token: Option<String>,
    pub data: Vec<Group>,
    pub object: ObjectType,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub external_id: Option<String>,
    pub id: String,
    pub name: String,
    pub object: ObjectType,
    pub organization_id: String,
    #[serde(default)]
    pub collections: Vec<Collection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: String,
    pub read_only: bool,
    pub hide_passwords: bool,
    pub manage: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserList {
    pub continuation_token: Option<String>,
    pub data: Vec<User>,
    pub object: ObjectType,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub access_all: bool,
    pub access_secrets_manager: bool,
    pub avatar_color: Option<String>,
    pub claimed_by_organization: bool,
    pub collections: Vec<String>,
    pub email: String,
    pub external_id: Option<String>,
    pub groups: Vec<String>,
    pub has_master_password: bool,
    pub id: Option<String>,
    pub managed_by_organization: bool,
    pub name: Option<String>,
    pub object: ObjectType,
    pub permissions: Option<String>,
    pub reset_password_enrolled: bool,
    pub sso_bound: bool,
    pub status: u32,
    pub two_factor_enabled: bool,
    pub r#type: u32,
    pub user_id: String,
    pub uses_key_connector: bool,
}

#[derive(Debug, Serialize)]
pub struct GroupSync {
    pub name: String,
    pub access_all: bool,
    pub external_id: Option<String>,
    pub collections: Vec<Collection>,
    pub users: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteData {
    pub emails: Vec<String>,
    pub groups: Vec<String>,
    pub r#type: u32, // 1 should be user
    pub collections: Option<Vec<String>>,
    pub permissions: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ObjectType {
    List,
    Group,
    GroupDetails,
    OrganizationUserUserDetails,
}
