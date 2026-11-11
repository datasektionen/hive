use std::collections::BTreeSet;

use chrono::{Months, NaiveDate};
use rocket::{State, serde::json::Json};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    api::HiveApiPermission,
    darkmode::DarkmodeClient,
    dto::{
        groups::{AddMemberDto, GroupRefDto},
        tags::{AssignTagToGroupDto, AssignTagWithApiDto},
    },
    errors::{AppError, AppResult},
    guards::{actor::Actor, api::consumer::ApiConsumer, lang::Language, perms::PermsEvaluator},
    models::{AffiliatedTagAssignment, GroupMember},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::{
        groups::{self, AuthorityInGroup},
        tags,
    },
};

pub fn routes() -> RouteTree {
    rocket::routes![
        tagged_groups,
        tagged_users,
        tagged_user_memberships,
        tagged_group_members,
        tagged_group_members_detailed,
        add_tagged_member,
        delete_tagged_member,
        tagged_group_content,
        add_tagged_group_content,
        delete_tagged_group_content
    ]
    .into()
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct TaggedGroup {
    group_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_description: Option<String>,
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
            group_description: assignment.description,
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

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct DetailedMember {
    id: Option<Uuid>,
    username: String,
    manager: bool,
    from: NaiveDate,
    until: NaiveDate,
}

impl From<GroupMember> for DetailedMember {
    fn from(value: GroupMember) -> Self {
        Self {
            id: value.id,
            username: value.username,
            manager: value.manager,
            from: value.from,
            until: value.until,
        }
    }
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

#[rocket::get("/group/<group_domain>/<group_id>/members")]
async fn tagged_group_members(
    group_id: &str,
    group_domain: &str,
    consumer: ApiConsumer,
    darkmode: &State<Option<DarkmodeClient>>,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<String>>> {
    consumer
        .require(HiveApiPermission::ListTagged, db.inner())
        .await?;

    let tagged_for_system =
        groups::tags::is_tagged_for_system(group_id, group_domain, &consumer.system_id, db.inner())
            .await?;
    if !tagged_for_system {
        return Err(AppError::NotAllowed(HivePermission::ApiListTagged));
    }

    let hidden_users = groups::members::get_hidden_members(darkmode, db.inner()).await?;

    let members = groups::members::get_all_members(group_id, group_domain, db.inner(), None)
        .await?
        .into_iter()
        .filter_map(|member| {
            if hidden_users.get(&member.username).is_none() {
                Some(member.username)
            } else {
                None
            }
        })
        .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(members))
}

#[rocket::get("/group/<group_domain>/<group_id>/members/details")]
async fn tagged_group_members_detailed(
    group_id: &str,
    group_domain: &str,
    consumer: ApiConsumer,
    darkmode: &State<Option<DarkmodeClient>>,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<DetailedMember>>> {
    consumer
        .require(HiveApiPermission::ListTagged, db.inner())
        .await?;

    let tagged_for_system =
        groups::tags::is_tagged_for_system(group_id, group_domain, &consumer.system_id, db.inner())
            .await?;
    if !tagged_for_system {
        return Err(AppError::NotAllowed(HivePermission::ApiListTagged));
    }

    let hidden_users = groups::members::get_hidden_members(darkmode, db.inner()).await?;

    let mut direct_members = groups::members::get_direct_members(
        group_id,
        group_domain,
        false,
        None::<Months>,
        db.inner(),
        None,
    )
    .await?;

    let sub_groups =
        groups::members::get_direct_subgroups(group_id, group_domain, db.inner()).await?;

    let mut indirect_members = Vec::new();

    for sub_group in sub_groups {
        let mut sub_members = groups::members::get_all_members(
            &sub_group.group.id,
            &sub_group.group.domain,
            db.inner(),
            None,
        )
        .await?;

        indirect_members.append(&mut sub_members);
    }

    direct_members.append(&mut indirect_members);

    let members = direct_members
        .into_iter()
        .filter_map(|member| {
            if hidden_users.get(&member.username).is_none() {
                Some(member)
            } else {
                None
            }
        })
        .map(DetailedMember::from)
        .collect(); // BTreeSet orders and removes duplicates

    Ok(Json(members))
}

#[rocket::post("/group/<group_domain>/<group_id>/members", data = "<dto>")]
async fn add_tagged_member<'v>(
    group_id: &str,
    group_domain: &str,
    dto: Json<AddMemberDto<'v>>,
    actor: Actor,
    perms: &PermsEvaluator,
    db: &State<PgPool>,
) -> AppResult<Json<DetailedMember>> {
    groups::details::require_authority(
        AuthorityInGroup::ManageMembers,
        group_id,
        group_domain,
        db.inner(),
        perms,
        &actor,
    )
    .await?;

    let result =
        groups::members::add_member(group_id, group_domain, &dto, db.inner(), None, &actor).await?;

    Ok(Json(result.into()))
}

#[rocket::delete("/group/<group_domain>/<group_id>/members/<membership_id>")]
async fn delete_tagged_member(
    group_id: &str,
    group_domain: &str,
    membership_id: &str,
    actor: Actor,
    perms: &PermsEvaluator,
    db: &State<PgPool>,
) -> AppResult<()> {
    groups::details::require_authority(
        AuthorityInGroup::ManageMembers,
        group_id,
        group_domain,
        db.inner(),
        perms,
        &actor,
    )
    .await?;

    groups::members::remove_member(
        &Uuid::try_parse(membership_id).unwrap(),
        group_id,
        group_domain,
        db.inner(),
        &actor,
    )
    .await?;

    Ok(())
}

#[rocket::get("/group/<group_domain>/<group_id>/tag/<tag_id>")]
async fn tagged_group_content(
    group_id: &str,
    group_domain: &str,
    tag_id: &str,
    consumer: ApiConsumer,
    db: &State<PgPool>,
) -> AppResult<Json<BTreeSet<String>>> {
    consumer
        .require(HiveApiPermission::ListTagged, db.inner())
        .await?;

    let content = groups::tags::list_tagged_content(
        group_id,
        group_domain,
        &consumer.system_id,
        tag_id,
        db.inner(),
    )
    .await?
    .into_iter()
    .collect();

    Ok(Json(content))
}

#[rocket::post("/group/<group_domain>/<group_id>/tag?<lang>", data = "<tag>")]
async fn add_tagged_group_content<'v>(
    group_id: &str,
    group_domain: &str,
    tag: Json<AssignTagWithApiDto<'v>>,
    lang: Option<Language>,
    consumer: ApiConsumer,
    actor: Actor,
    perms: &PermsEvaluator,
    db: &State<PgPool>,
) -> AppResult<Json<TaggedGroup>> {
    let min = HivePermission::AssignTags(SystemsScope::Id(consumer.system_id.to_string()));
    perms.require(min).await?;

    let _ = tags::require_one(&consumer.system_id.to_string(), &tag.tag, db.inner()).await?;

    let dto = AssignTagToGroupDto {
        group: GroupRefDto {
            id: group_id,
            domain: group_domain,
        },
        content: tag.content,
    };

    let assignment = tags::assign_to_group(
        &consumer.system_id.to_string(),
        &tag.tag,
        &dto,
        lang.as_ref(),
        db.inner(),
        &actor,
    )
    .await?;

    Ok(Json(assignment.into()))
}

// Should add an endpoint to remove tags without content

#[rocket::delete("/group/<group_domain>/<group_id>/tag/<tag_id>/content/<tag_content>")]
async fn delete_tagged_group_content(
    group_id: &str,
    group_domain: &str,
    tag_id: &str,
    tag_content: &str,
    consumer: ApiConsumer,
    actor: Actor,
    perms: &PermsEvaluator,
    db: &State<PgPool>,
) -> AppResult<()> {
    let min = HivePermission::ManageTags(SystemsScope::Id(consumer.system_id.clone()));
    perms.require(min).await?;

    tags::unassign_with_content(
        group_id,
        group_domain,
        &consumer.system_id.clone(),
        tag_id,
        tag_content,
        db.inner(),
        perms,
        &actor,
    )
    .await?;

    Ok(())
}
