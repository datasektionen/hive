use rocket::{
    http::{uri::Host, CookieJar},
    response::Redirect,
    State,
};
use sqlx::PgPool;

use crate::{
    auth::{self, oidc::OidcClient},
    errors::AppResult,
    guards::scheme::RequestScheme,
    routing::RouteTree,
    services::groups,
};

pub fn routes() -> RouteTree {
    rocket::routes![login, oidc_callback, logout].into()
}

#[rocket::get("/auth/login")]
async fn login(
    oidc_client: &State<OidcClient>,
    scheme: RequestScheme,
    host: &Host<'_>,
    jar: &CookieJar<'_>,
) -> AppResult<Redirect> {
    let url = if auth::get_current_user(jar).is_some() {
        "/".to_owned()
    } else {
        auth::begin_authentication(
            format!("{scheme}://{host}/auth/oidc-callback"),
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
    let user = auth::finish_authentication(code, state, oidc_client, jar).await?;

    groups::members::conditional_bootstrap(&user.username, db.inner()).await?;

    Ok(Redirect::to("/groups"))
}

#[rocket::get("/auth/logout")]
async fn logout(jar: &CookieJar<'_>) -> AppResult<Redirect> {
    auth::logout(jar);

    Ok(Redirect::to("/"))
}
