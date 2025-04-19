use rocket::{
    http::{
        uri::{Host, Origin},
        CookieJar,
    },
    response::Redirect,
    State,
};
use sqlx::PgPool;

use crate::{
    auth::{
        self,
        oidc::{OidcAuthenticationResult, OidcClient},
    },
    errors::AppResult,
    guards::scheme::RequestScheme,
    routing::RouteTree,
    services::groups,
};

pub fn routes() -> RouteTree {
    rocket::routes![login, oidc_callback, logout].into()
}

#[rocket::get("/auth/login?<next>")]
async fn login(
    next: Option<&str>,
    oidc_client: &State<OidcClient>,
    scheme: RequestScheme,
    host: &Host<'_>,
    jar: &CookieJar<'_>,
) -> AppResult<Redirect> {
    let next = next.and_then(|path| Origin::parse(path).ok());

    let url = if auth::get_current_session(jar).is_some() {
        next.as_ref()
            .map(Origin::to_string)
            .unwrap_or_else(|| "/groups".to_owned())
    } else {
        auth::begin_authentication(
            format!("{scheme}://{host}/auth/oidc-callback"),
            next,
            oidc_client,
            jar,
        )
        .await?
    };

    Ok(Redirect::to(url))
}

#[rocket::get("/auth/oidc-callback?<code>&<state>")]
async fn oidc_callback(
    code: &str,
    state: &str,
    oidc_client: &State<OidcClient>,
    db: &State<PgPool>,
    jar: &CookieJar<'_>,
) -> AppResult<Redirect> {
    let OidcAuthenticationResult { session, next } =
        auth::finish_authentication(code, state, oidc_client, jar).await?;

    groups::members::conditional_bootstrap(&session.username, db.inner()).await?;

    let target = next.unwrap_or_else(|| Origin::parse("/groups").unwrap());

    Ok(Redirect::to(target))
}

#[rocket::get("/auth/logout")]
async fn logout(jar: &CookieJar<'_>) -> AppResult<Redirect> {
    auth::logout(jar);

    Ok(Redirect::to("/"))
}
