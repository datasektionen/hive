use chrono::{DateTime, Local};
use log::*;
use oidc::{OidcAuthenticationResult, OidcClient};
use rocket::http::{uri::Origin, Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};

use crate::errors::{AppError, AppResult};

pub mod oidc;

// can't be __Host- because it would not work on http://localhost in Chrome
const LOGIN_FLOW_CONTEXT_COOKIE: &str = "Hive-Login-Flow-Context";
const AUTH_COOKIE: &str = "Hive-Auth";

#[derive(Serialize, Deserialize)]
pub struct Session {
    pub username: String,
    pub display_name: String,
    pub expiration: DateTime<Local>,
}

pub async fn begin_authentication(
    redirect_url: String,
    next: Option<Origin<'_>>,
    oidc_client: &OidcClient,
    jar: &CookieJar<'_>,
) -> AppResult<String> {
    let (url, context) = oidc_client.begin_authentication(redirect_url, next).await?;

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
) -> AppResult<OidcAuthenticationResult<'static>> {
    let cookie = jar
        .get_private(LOGIN_FLOW_CONTEXT_COOKIE)
        .ok_or(AppError::AuthenticationFlowExpired)?;

    let context = serde_json::from_str(cookie.value_trimmed())
        .map_err(AppError::StateDeserializationError)?;

    let result = oidc_client
        .finish_authentication(context, code, state)
        .await?;

    let session = &result.session;

    debug!("User {} logged in successfully", session.username);

    let value = serde_json::to_string(&session).map_err(AppError::StateSerializationError)?;

    // easier to set max age than expires because chrono =/= time
    let delta = session.expiration - Local::now();

    let cookie = Cookie::build((AUTH_COOKIE, value))
        .secure(true)
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(rocket::time::Duration::seconds(delta.num_seconds()));
    // ^ we ignore sub-sec nanoseconds for simplicity

    jar.add_private(cookie);
    jar.remove_private(LOGIN_FLOW_CONTEXT_COOKIE);

    Ok(result)
}

pub fn get_current_session(jar: &CookieJar<'_>) -> Option<Session> {
    if let Some(cookie) = jar.get_private(AUTH_COOKIE) {
        if let Ok(session) = serde_json::from_str::<Session>(cookie.value_trimmed()) {
            if session.expiration >= Local::now() {
                return Some(session);
            }
        }
    }

    None
}

pub fn logout(jar: &CookieJar<'_>) {
    jar.remove_private(AUTH_COOKIE);
}

pub fn impersonate(
    target_username: String,
    target_display_name: String,
    jar: &CookieJar<'_>,
) -> AppResult<()> {
    if let Some(mut session) = get_current_session(jar) {
        session.username = target_username;
        session.display_name = target_display_name;

        let value = serde_json::to_string(&session).map_err(AppError::StateSerializationError)?;

        if let Some(mut cookie) = jar.get_private(AUTH_COOKIE) {
            cookie.set_value(value);

            jar.add_private(cookie);
        }
    }

    Ok(())
}
