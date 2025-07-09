use std::{collections::HashMap, fmt};

use chrono::{Duration, Utc};
use log::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

// space-separated list of permissions required
// (options: https://developers.google.com/identity/protocols/oauth2/scopes)
// Note: these must also be authorized under Domain-Wide Delegation from Google
// Workspace admin panel at https://admin.google.com/ac/owl/domainwidedelegation
const SCOPE: &str = concat!(
    "https://www.googleapis.com/auth/admin.directory.user",
    " https://www.googleapis.com/auth/admin.directory.group",
    " https://www.googleapis.com/auth/apps.groups.settings",
);

const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
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

        let treated = response
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                error!("Directory API failed to execute request ({url}): {e:?}");
                error!("Sent body: {body:?}");

                error_message
            })?;

        if method == reqwest::Method::DELETE {
            // no response to decode
            return Ok(None);
        }

        let decoded = treated.json().await.map_err(|e| {
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

    pub async fn create_group(&self, group: &NewGroup) -> Result<SimpleGroup, &'static str> {
        self.exec_request(
            reqwest::Method::POST,
            "https://admin.googleapis.com/admin/directory/v1/groups",
            Some(group),
            "Failed to create group",
        )
        .await
        .and_then(|op| op.ok_or("Failed to create group"))
    }

    pub async fn delete_group(&self, key: &str) -> Result<Option<()>, &'static str> {
        self.exec_request(
            reqwest::Method::DELETE,
            &format!("https://admin.googleapis.com/admin/directory/v1/groups/{key}"),
            None::<()>,
            "Failed to delete group",
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

    pub async fn get_group_settings(
        &self,
        key: &str,
    ) -> Result<Option<GroupSettings>, &'static str> {
        self.exec_request(
            reqwest::Method::GET,
            &format!("https://www.googleapis.com/groups/v1/groups/{key}?alt=json"),
            // ^ yes, of course the group settings API defaults to XML format...
            None::<()>,
            "Failed to get group settings",
        )
        .await
    }

    pub async fn patch_group_settings(
        &self,
        key: &str,
        patch: &GroupSettingsPatch<'_>,
    ) -> Result<Option<GroupSettings>, &'static str> {
        self.exec_request(
            reqwest::Method::PATCH,
            &format!("https://www.googleapis.com/groups/v1/groups/{key}?alt=json"),
            Some(patch),
            "Failed to patch group settings",
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

#[derive(Debug, Serialize)]
pub struct NewGroup {
    pub email: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct GroupMemberPatch {
    pub role: GroupMemberRole,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSettings {
    pub name: String,
    pub description: String,                      // max 4096 characters
    pub who_can_view_group: GroupVisibility,      // view messages
    pub who_can_view_membership: GroupVisibility, // view members list
    pub who_can_discover_group: GroupDiscoverability,
    pub who_can_join: GroupJoinPermission,
    pub who_can_leave_group: GroupLeavePermission,
    pub who_can_contact_owner: GroupContactOwnerPermission,
    pub who_can_post_message: GroupPostPermission,
    pub who_can_moderate_members: GroupModerationPermission,
    pub who_can_moderate_content: GroupModerationPermission,
    pub who_can_assist_content: GroupModerationPermission,
    pub allow_web_posting: PoorMansBoolean,
    pub allow_external_members: PoorMansBoolean,
    pub is_archived: PoorMansBoolean, // message history is kept
    pub members_can_post_as_the_group: PoorMansBoolean,
    pub enable_collaborative_inbox: PoorMansBoolean,
    pub message_moderation_level: GroupMessageModerationLevel,
    pub spam_moderation_level: GroupSpamModerationLevel,
    pub default_sender: GroupDefaultSender,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSettingsPatch<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_view_group: Option<GroupVisibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_view_membership: Option<GroupVisibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_discover_group: Option<GroupDiscoverability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_join: Option<GroupJoinPermission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_leave_group: Option<GroupLeavePermission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_contact_owner: Option<GroupContactOwnerPermission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_post_message: Option<GroupPostPermission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_moderate_members: Option<GroupModerationPermission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_moderate_content: Option<GroupModerationPermission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_can_assist_content: Option<GroupModerationPermission>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_web_posting: Option<PoorMansBoolean>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_external_members: Option<PoorMansBoolean>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_archived: Option<PoorMansBoolean>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members_can_post_as_the_group: Option<PoorMansBoolean>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_collaborative_inbox: Option<PoorMansBoolean>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_moderation_level: Option<GroupMessageModerationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spam_moderation_level: Option<GroupSpamModerationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sender: Option<GroupDefaultSender>,
}

impl<'a> GroupSettingsPatch<'a> {
    pub fn new(
        current: &GroupSettings,
        target: &'a GroupSettings,
        alt_name: &str,
        alt_description: &str,
    ) -> Option<Self> {
        let name = (current.name != target.name && current.name != alt_name)
            .then_some(target.name.as_str());

        let description = (current.description != target.description
            && current.description != alt_description)
            .then_some(target.description.as_str());

        // only works for Copy types, so not name/description
        macro_rules! field_diff {
            ($key:ident) => {
                let $key = (current.$key != target.$key).then_some(target.$key);
            };
        }

        field_diff!(who_can_view_group);
        field_diff!(who_can_view_membership);
        field_diff!(who_can_discover_group);
        field_diff!(who_can_join);
        field_diff!(who_can_leave_group);
        field_diff!(who_can_contact_owner);
        field_diff!(who_can_post_message);
        field_diff!(who_can_moderate_members);
        field_diff!(who_can_moderate_content);
        field_diff!(who_can_assist_content);
        field_diff!(allow_web_posting);
        field_diff!(allow_external_members);
        field_diff!(is_archived);
        field_diff!(members_can_post_as_the_group);
        field_diff!(enable_collaborative_inbox);
        field_diff!(message_moderation_level);
        field_diff!(spam_moderation_level);
        field_diff!(default_sender);

        let patch = Self {
            name,
            description,
            who_can_view_group,
            who_can_view_membership,
            who_can_discover_group,
            who_can_join,
            who_can_leave_group,
            who_can_contact_owner,
            who_can_post_message,
            who_can_moderate_members,
            who_can_moderate_content,
            who_can_assist_content,
            allow_web_posting,
            allow_external_members,
            is_archived,
            members_can_post_as_the_group,
            enable_collaborative_inbox,
            message_moderation_level,
            spam_moderation_level,
            default_sender,
        };

        if let Ok(serde_json::Value::Object(map)) = serde_json::to_value(&patch) {
            if map.is_empty() {
                return None;
            }
        }

        Some(patch)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub enum GroupVisibility {
    AllInDomainCanView,
    AllMembersCanView,
    AllManagersCanView,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub enum GroupDiscoverability {
    AnyoneCanDiscover,
    AllInDomainCanDiscover,
    AllMembersCanDiscover,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub enum GroupJoinPermission {
    AnyoneCanJoin,
    AllInDomainCanJoin,
    InvitedCanJoin,
    CanRequestToJoin,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub enum GroupLeavePermission {
    AllManagersCanLeave,
    AllMembersCanLeave,
    NoneCanLeave,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub enum GroupContactOwnerPermission {
    AllInDomainCanContact,
    AllManagersCanContact,
    AllMembersCanContact,
    AnyoneCanContact,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub enum GroupPostPermission {
    NoneCanPost,
    AllManagersCanPost,
    AllMembersCanPost,
    AllOwnersCanPost,
    AllInDomainCanPost,
    AnyoneCanPost,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupModerationPermission {
    AllMembers,
    OwnersAndManagers,
    OwnersOnly,
    None,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[allow(clippy::enum_variant_names)]
pub enum GroupMessageModerationLevel {
    ModerateAllMessages,
    ModerateNonMembers,
    ModerateNewMembers,
    ModerateNone,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupSpamModerationLevel {
    Allow,
    Moderate,
    SilentlyModerate,
    Reject,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroupDefaultSender {
    DefaultSelf,
    Group,
}

// of course Google Group Settings API doesn't use actual JSON booleans,
// but rather string values that can be either "true" or "false"...
// Serde doesn't seem to have a great way to deal with that conversion
// on (de)serialization without a lot of extra complexity, so we just
// use a custom enum
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PoorMansBoolean {
    True,
    False,
}

impl From<bool> for PoorMansBoolean {
    fn from(value: bool) -> Self {
        if value {
            Self::True
        } else {
            Self::False
        }
    }
}

impl fmt::Debug for PoorMansBoolean {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        (*self == Self::True).fmt(f)
    }
}
