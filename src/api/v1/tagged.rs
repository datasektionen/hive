use std::collections::BTreeSet;

use rocket::{State, serde::json::Json};
use sqlx::PgPool;

use crate::{
    api::{
        HiveApiPermission,
        v1::{TaggedGroup, TaggedUser},
    },
    darkmode::DarkmodeClient,
    errors::AppResult,
    guards::{api::consumer::ApiConsumer, lang::Language},
    routing::RouteTree,
    services::{groups, tags},
};

pub fn routes() -> RouteTree {
    rocket::routes![tagged_groups, tagged_users, tagged_user_memberships].into()
}

#[rocket::get("/tagged/<tag_id>/groups?<lang>&<description>")]
async fn tagged_groups(
    tag_id: &str,
    lang: Option<Language>,
    description: Option<bool>,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<TaggedGroup>>> {
    consumer
        .require(HiveApiPermission::ListTagged, db.inner())
        .await?;

    let lang = lang.unwrap_or(Language::Swedish);

    let assignments = tags::list_group_assignments(
        &consumer.system_id,
        tag_id,
        Some(&lang),
        None,
        db.inner(),
        None,
        description.unwrap_or_default(),
    )
    .await?
    .into_iter()
    .map(Into::into)
    .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(assignments))
}

#[rocket::get("/tagged/<tag_id>/users")]
async fn tagged_users(
    tag_id: &str,
    consumer: ApiConsumer,
    darkmode: &State<Option<DarkmodeClient>>,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<TaggedUser>>> {
    consumer
        .require(HiveApiPermission::ListTagged, db.inner())
        .await?;

    let hidden_users = groups::members::get_hidden_members(darkmode, db.inner()).await?;

    let assignments =
        tags::list_user_assignments(&consumer.system_id, tag_id, db.inner(), None, None)
            .await?
            .into_iter()
            .filter_map(|tag| {
                if let Some(ref username) = tag.username
                    && hidden_users.get(username).is_none()
                {
                    Some(tag.into())
                } else {
                    None
                }
            })
            .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(assignments))
}

#[rocket::get("/tagged/<tag_id>/memberships/<username>?<lang>&<description>")]
async fn tagged_user_memberships(
    tag_id: &str,
    username: &str,
    lang: Option<Language>,
    description: Option<bool>,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<TaggedGroup>>> {
    consumer
        .require(HiveApiPermission::ListTagged, db.inner())
        .await?;

    let lang = lang.unwrap_or(Language::Swedish);

    let assignments = tags::list_group_assignments(
        &consumer.system_id,
        tag_id,
        Some(&lang),
        Some(username),
        db.inner(),
        None,
        description.unwrap_or_default(),
    )
    .await?
    .into_iter()
    .map(Into::into)
    .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(assignments))
}
