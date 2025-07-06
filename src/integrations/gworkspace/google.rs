use std::{collections::HashMap, fmt};

use chrono::{Duration, Utc};
use log::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

// space-separated list of permissions required
// (options: https://developers.google.com/identity/protocols/oauth2/scopes)
const SCOPE: &str = concat!(
    "https://www.googleapis.com/auth/admin.directory.user",
    " https://www.googleapis.com/auth/admin.directory.group"
);

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const USER_AGENT: &str = "hive-gworkspace-integration";

/// Note that after construction, the client expires after 1h.
///
/// A more complex system with auto-reconstruction could be implemented in the
/// future, but for now it's not necessary since the client is only used for
/// a few seconds at most.
pub struct DirectoryApiClient {
    reqwest_client: reqwest::Client,
    access_token: String,
}

impl DirectoryApiClient {
    pub async fn new(
        service_account_email: &str,
        private_key: &str,
        impersonate_user: &str,
    ) -> Result<Self, &'static str> {
        let reqwest_client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                error!("Directory API failed to build reqwest client: {e}");

                "Failed to build Reqwest client"
            })?;

        let token_details = Self::get_access_token(
            &reqwest_client,
            service_account_email,
            private_key,
            impersonate_user,
        )
        .await?;

        debug!(
            "Obtained access token of type `{}` expiring in {} seconds",
            token_details.token_type, token_details.expires_in
        );

        Ok(Self {
            reqwest_client,
            access_token: token_details.access_token,
        })
    }

    async fn get_access_token(
        reqwest_client: &reqwest::Client,
        service_account_email: &str,
        private_key: &str,
        impersonate_user: &str,
    ) -> Result<AccessTokenResponse, &'static str> {
        let jwt = Self::create_jwt(service_account_email, private_key, impersonate_user)?;

        let mut params = HashMap::new();
        params.insert("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer");
        params.insert("assertion", &jwt);

        let response: AccessTokenResponse = reqwest_client
            .post("https://oauth2.googleapis.com/token")
            .form(&params)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                error!("Directory API failed to get access token: {e}");

                "Failed to get access token"
            })?
            .json()
            .await
            .map_err(|e| {
                error!("Directory API failed to decode access token JSON: {e}");

                "Failed to decode access token JSON"
            })?;

        Ok(response)
    }

    fn create_jwt(
        service_account_email: &str,
        private_key: &str,
        impersonate_user: &str,
    ) -> Result<String, &'static str> {
        let header = jsonwebtoken::Header {
            typ: Some("JWT".to_owned()),
            alg: jsonwebtoken::Algorithm::RS256,
            ..Default::default()
        };

        let iat = Utc::now();
        let exp = iat + Duration::hours(1);

        let claims = JwtClaims {
            iss: service_account_email,
            aud: "https://oauth2.googleapis.com/token",
            iat: iat.timestamp(),
            exp: exp.timestamp(),
            sub: impersonate_user,
            scope: SCOPE,
        };

        let key = jsonwebtoken::EncodingKey::from_rsa_pem(private_key.as_bytes()).map_err(|e| {
            error!("JWT invalid RSA key: {e}");

            "Failed to decode RSA key"
        })?;

        jsonwebtoken::encode(&header, &claims, &key).map_err(|e| {
            error!("JWT encoding failure: {e}");

            "Failed to encode JWT"
        })
    }

    async fn exec_request<R: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        url: impl reqwest::IntoUrl + Copy + fmt::Display,
        body: Option<impl Serialize>,
        error_message: &'static str,
    ) -> Result<Option<R>, &'static str> {
        let request = self
            .reqwest_client
            .request(method, url)
            .bearer_auth(&self.access_token);

        let request = if let Some(body) = body {
            request.json(&body)
        } else {
            request
        };

        let response = request.send().await;

        if let Ok(response) = &response {
            if matches!(
                response.status(),
                reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::NOT_FOUND
            ) {
                // for groups (not users), API doesn't return 404, but 403
                // (we assume we have sufficient permissions for anything)
                return Ok(None);
            }
        }

        let decoded = response
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                error!("Directory API failed to execute request ({url}): {e:?}");

                error_message
            })?
            .json()
            .await
            .map_err(|e| {
                error!("Directory API failed to decode response JSON ({url}): {e:?}");

                "Failed to decode response JSON"
            })?;

        Ok(Some(decoded))
    }

    async fn paginated_list<R: DeserializeOwned>(
        &self,
        url: impl reqwest::IntoUrl + Copy,
        mut params: HashMap<&'static str, String>,
        key: &str,
        error_message: &'static str,
    ) -> Result<Vec<R>, &'static str> {
        let mut items = vec![];

        params.insert("maxResult", "200".to_owned());

        loop {
            let response: serde_json::Value = self
                .reqwest_client
                .get(url)
                .bearer_auth(&self.access_token)
                .query(&params)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| {
                    error!("Directory API failed to execute paginated request: {e:?}");

                    error_message
                })?
                .json()
                .await
                .map_err(|e| {
                    error!("Directory API failed to decode paginated response JSON: {e:?}");

                    "Failed to decode paginated response JSON"
                })?;

            if let Some(obj) = response.as_object() {
                if let Some(serde_json::Value::Array(values)) = obj.get(key) {
                    items.extend(
                        values
                            .iter()
                            .cloned()
                            .filter_map(|v| serde_json::from_value(v).ok()),
                    );
                }

                // apparently it's not an error if the object doesn't contain `key`,
                // it's just equivalent to an empty array, so we do nothing...

                if let Some(serde_json::Value::String(token)) = obj.get("nextPageToken") {
                    params.insert("pageToken", token.clone());
                } else {
                    break;
                }
            }
        }

        Ok(items)
    }

    pub async fn get_user(&self, key: &str) -> Result<Option<User>, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!(
                "https://admin.googleapis.com/admin/directory/v1/users/{key}?projection=BASIC&viewType=admin_view"
            ),
            None::<()>,
            "Failed to get user",
        )
        .await
    }

    pub async fn list_groups(&self) -> Result<Vec<SimpleGroup>, &'static str> {
        let params = HashMap::from([("customer", "my_customer".to_owned())]);

        self.paginated_list(
            "https://admin.googleapis.com/admin/directory/v1/groups",
            params,
            "groups",
            "Failed to list groups",
        )
        .await
    }

    pub async fn create_group(&self, group: &NewGroup) -> Result<Group, &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            "https://admin.googleapis.com/admin/directory/v1/groups",
            Some(group),
            "Failed to create group",
        )
        .await
        .and_then(|op| op.ok_or("Failed to create group"))
    }

    pub async fn get_group(&self, key: &str) -> Result<Option<Group>, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!("https://admin.googleapis.com/admin/directory/v1/groups/{key}"),
            None::<()>,
            "Failed to get group",
        )
        .await
    }

    pub async fn patch_group(
        &self,
        key: &str,
        patch: &GroupPatch<'_>,
    ) -> Result<Option<Group>, &'static str> {
        self.exec_request(
            reqwest::Method::PATCH,
            &format!("https://admin.googleapis.com/admin/directory/v1/groups/{key}"),
            Some(patch),
            "Failed to patch group",
        )
        .await
    }

    pub async fn list_group_members(&self, key: &str) -> Result<Vec<GroupMember>, &'static str> {
        let params = HashMap::from([("includeDerivedMembership", "false".to_owned())]);

        self.paginated_list(
            &format!("https://admin.googleapis.com/admin/directory/v1/groups/{key}/members"),
            params,
            "members",
            "Failed to list group members",
        )
        .await
    }

    pub async fn add_group_member(
        &self,
        group_key: &str,
        member: &GroupMember,
    ) -> Result<Option<GroupMember>, &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            &format!("https://admin.googleapis.com/admin/directory/v1/groups/{group_key}/members"),
            Some(member),
            "Failed to add group member",
        )
        .await
    }

    pub async fn remove_group_member(
        &self,
        group_key: &str,
        member_key: &str,
    ) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::DELETE,
            &format!("https://admin.googleapis.com/admin/directory/v1/groups/{group_key}/members/{member_key}"),
            None::<()>,
            "Failed to delete group member",
        )
        .await
    }

    pub async fn patch_group_member(
        &self,
        group_key: &str,
        member_key: &str,
        patch: &GroupMemberPatch,
    ) -> Result<Option<GroupMember>, &'static str> {
        self.exec_request(
            reqwest::Method::PATCH,
            &format!("https://admin.googleapis.com/admin/directory/v1/groups/{group_key}/members/{member_key}"),
            Some(patch),
            "Failed to patch group member",
        )
        .await
    }
}

#[derive(Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
    sub: &'a str,
    scope: &'a str,
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
    token_type: String,
    expires_in: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub primary_email: String,
}

#[derive(Debug, Deserialize)]
pub struct SimpleGroup {
    pub email: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct NewGroup {
    pub email: String,
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct GroupPatch<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GroupMember {
    pub email: String,
    pub role: GroupMemberRole,
    pub r#type: GroupMemberType,
    #[serde(default)]
    pub delivery_settings: Option<GroupMemberDeliverySettings>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupMemberRole {
    Member,
    Manager,
    Owner,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupMemberType {
    Group,
    User,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupMemberDeliverySettings {
    AllMail,
    Daily,
    Digest,
    Disabled,
    None,
}

#[derive(Serialize)]
pub struct GroupMemberPatch {
    pub role: GroupMemberRole,
}
