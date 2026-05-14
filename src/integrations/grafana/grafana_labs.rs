use std::fmt;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

const USER_AGENT: &str = "hive-grafana-integration";

pub struct GrafanaApiClient {
    base_url: String,
    reqwest_client: reqwest::Client,
    api_token: String,
}

impl GrafanaApiClient {
    pub fn new(base_url: String, api_token: String) -> Result<Self, &'static str> {
        let reqwest_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                log::error!("Grafana API failed to build reqwest client: {e}");

                "Failed to build Reqwest client"
            })?;

        Ok(Self {
            base_url,
            reqwest_client,
            api_token,
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
            .bearer_auth(&self.api_token);

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
                log::error!("Grafana API failed to execute request ({url}): {e:?}");
                log::error!("Sent body: {body:?}");

                error_message
            })?;

        if method == reqwest::Method::DELETE {
            return Ok(None);
        }

        let decoded = response.json().await.map_err(|e| {
            log::error!("Grafana API failed to decode response JSON ({url}): {e:?}");

            "Failed to decode response JSON"
        })?;

        Ok(Some(decoded))
    }

    pub async fn list_teams(&self) -> Result<TeamList, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!("{}/api/teams/search", self.base_url),
            None::<()>,
            "Failed to list teams",
        )
        .await
        .and_then(|op| op.ok_or("Failed to list teams"))
    }

    pub async fn list_org_members(&self) -> Result<Vec<OrgUser>, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!("{}/api/org/users", self.base_url),
            None::<()>,
            "Failed to list members",
        )
        .await
        .and_then(|op| op.ok_or("Failed to list members"))
    }

    pub async fn list_team_members(&self, key: u32) -> Result<Vec<TeamMember>, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!("{}/api/teams/{key}/members", self.base_url),
            None::<()>,
            "Failed to list team members",
        )
        .await
        .and_then(|op| op.ok_or("Failed to list team members"))
    }

    pub async fn create_team(&self, body: CreateTeam) -> Result<CreateTeamResponse, &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            &format!("{}/api/teams", self.base_url),
            Some(body),
            "Failed to create team",
        )
        .await
        .and_then(|op| op.ok_or("Failed to create team"))
    }

    pub async fn sync_team_members(
        &self,
        key: u32,
        body: UpdateTeamMembers,
    ) -> Result<Option<GrafanaResponse>, &'static str> {
        self.exec_request(
            reqwest::Method::PUT,
            &format!("{}/api/teams/{key}/members", self.base_url),
            Some(body),
            "Failed to sync team members",
        )
        .await
    }

    pub async fn delete_team(&self, key: u32) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::DELETE,
            &format!("{}/api/teams/{key}", self.base_url),
            None::<()>,
            "Failed to delete team",
        )
        .await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrgUser {
    pub org_id: u32,
    pub user_id: u32,
    pub email: String,
    pub avatar_url: String,
    pub login: String,
    pub role: String,
    pub last_seen_at: String,
    pub last_seen_at_age: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamList {
    pub total_count: u32,
    pub teams: Vec<Team>,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: u32,
    pub org_id: u32,
    pub name: String,
    pub email: String,
    pub is_provisioned: bool,
    pub avatar_url: String,
    pub member_count: u32,
    pub permission: u32,
    pub access_control: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateTeam {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTeamResponse {
    pub message: String,
    pub team_id: u32,
    pub uid: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    pub org_id: u32,
    pub team_id: u32,
    pub user_id: u32,
    pub email: String,
    pub login: String,
    pub avatar_url: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateTeamMembers {
    pub members: Vec<String>,
    pub admins: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct GrafanaResponse {
    pub message: String,
}
