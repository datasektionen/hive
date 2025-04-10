use std::collections::BTreeSet;

use rocket::{serde::json::Json, State};
use serde::Serialize;
use sqlx::PgPool;

use crate::{
    api::HiveApiPermission,
    errors::AppResult,
    guards::{api::consumer::ApiConsumer, lang::Language},
    models::AffiliatedTagAssignment,
    routing::RouteTree,
    services::tags,
};

pub fn routes() -> RouteTree {
    rocket::routes![tagged_groups, tagged_users, tagged_user_memberships].into()
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct TaggedGroup {
    group_name: String,
    group_domain: String, // should be ordered first since it's shown separately
    group_id: String,
    tag_content: Option<String>,
}

impl From<AffiliatedTagAssignment> for TaggedGroup {
    fn from(assignment: AffiliatedTagAssignment) -> Self {
        Self {
            group_domain: assignment.group_domain.unwrap_or_default(),
            group_id: assignment.group_id.unwrap_or_default(),
            tag_content: assignment.content,
            group_name: assignment.label.unwrap_or_default(),
        }
    }
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct TaggedUser {
    username: String,
    tag_content: Option<String>,
}

impl From<AffiliatedTagAssignment> for TaggedUser {
    fn from(assignment: AffiliatedTagAssignment) -> Self {
        Self {
            username: assignment.username.unwrap_or_default(),
            tag_content: assignment.content,
        }
    }
}

#[rocket::get("/tagged/<tag_id>/groups?<lang>")]
async fn tagged_groups(
    tag_id: &str,
    lang: Option<Language>,
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
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<TaggedUser>>> {
    consumer
        .require(HiveApiPermission::ListTagged, db.inner())
        .await?;

    let assignments =
        tags::list_user_assignments(&consumer.system_id, tag_id, None, db.inner(), None)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(assignments))
}

#[rocket::get("/tagged/<tag_id>/memberships/<username>?<lang>")]
async fn tagged_user_memberships(
    tag_id: &str,
    username: &str,
    lang: Option<Language>,
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
    )
    .await?
    .into_iter()
    .map(Into::into)
    .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(assignments))
}
