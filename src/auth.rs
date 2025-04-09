use chrono::{DateTime, Local};
use log::*;
use oidc::OidcClient;
use rocket::http::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

pub mod oidc;

// can't be __Host- because it would not work on http://localhost in Chrome
const LOGIN_FLOW_CONTEXT_COOKIE: &str = "Hive-Login-Flow-Context";
const AUTH_COOKIE: &str = "Hive-Auth";

#[derive(Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub display_name: String,
    pub session_expires: DateTime<Local>,
}

pub async fn begin_authentication(
    redirect_url: String,
    oidc_client: &OidcClient,
    jar: &CookieJar<'_>,
) -> AppResult<String> {
    let (url, context) = oidc_client.begin_authentication(redirect_url).await?;

    let value = serde_json::to_string(&context).map_err(AppError::StateSerializationError)?;

    let cookie = Cookie::build((LOGIN_FLOW_CONTEXT_COOKIE, value))
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(rocket::time::Duration::minutes(5));

    jar.add_private(cookie);

    Ok(url)
}

pub async fn finish_authentication(
    code: &str,
    state: &str,
    oidc_client: &OidcClient,
    jar: &CookieJar<'_>,
) -> AppResult<User> {
    let cookie = jar
        .get_private(LOGIN_FLOW_CONTEXT_COOKIE)
        .ok_or(AppError::AuthenticationFlowExpired)?;

    let context = serde_json::from_str(cookie.value_trimmed())
        .map_err(AppError::StateDeserializationError)?;

    let user = oidc_client
        .finish_authentication(context, code, state)
        .await?;

    debug!("User {} logged in successfully", user.username);

    let value = serde_json::to_string(&user).map_err(AppError::StateSerializationError)?;

    // easier to set max age than expires because chrono =/= time
    let delta = user.session_expires - Local::now();

    let cookie = Cookie::build((AUTH_COOKIE, value))
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(rocket::time::Duration::seconds(delta.num_seconds()));
    // ^ we ignore sub-sec nanoseconds for simplicity

    jar.add_private(cookie);
    jar.remove_private(LOGIN_FLOW_CONTEXT_COOKIE);

    Ok(user)
}

pub fn get_current_user(jar: &CookieJar<'_>) -> Option<User> {
    if let Some(cookie) = jar.get_private(AUTH_COOKIE) {
        if let Ok(user) = serde_json::from_str::<User>(cookie.value_trimmed()) {
            if user.session_expires >= Local::now() {
                return Some(user);
            }
        }
    }

    None
}

pub fn logout(jar: &CookieJar<'_>) {
    jar.remove_private(AUTH_COOKIE);
}
