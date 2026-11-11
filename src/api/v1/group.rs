use std::collections::BTreeSet;

use chrono::Months;
use rocket::{State, serde::json::Json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    api::{HiveApiPermission, v1::{DetailedMember, TaggedGroup}}, darkmode::DarkmodeClient, dto::{
        groups::{AddMemberDto, GroupRefDto},
        tags::{AssignTagToGroupDto, AssignTagWithApiDto},
    }, errors::{AppError, AppResult}, guards::{actor::Actor, api::consumer::ApiConsumer, lang::Language, perms::PermsEvaluator}, perms::{HivePermission, SystemsScope}, routing::RouteTree, services::{
        groups::{self, AuthorityInGroup},
        tags,
    },
};

pub fn routes() -> RouteTree {
    rocket::routes![
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
