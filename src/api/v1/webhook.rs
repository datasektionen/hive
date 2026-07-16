use rocket::State;

use crate::{
    DarkmodeClient, errors::AppResult, guards::headers::DarkmodeEvent, routing::RouteTree,
};

pub fn routes() -> RouteTree {
    rocket::routes![darkmode].into()
}

#[rocket::get("/webhook/darkmode")]
async fn darkmode(
    header: Option<DarkmodeEvent<'_>>,
    darkmode: &State<Option<DarkmodeClient>>,
) -> AppResult<()> {
    if header.is_some() {
        if let Some(darkmode) = darkmode.as_ref() {
            darkmode.update_state().await?;
        }
    }

    Ok(())
}
