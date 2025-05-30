use chrono::Local;
use serde_json::json;
use uuid::Uuid;

use super::{audit_logs, pg_args};
use crate::{
    dto::tags::{AssignTagToGroupDto, AssignTagToUserDto, CreateSubtagDto, CreateTagDto},
    errors::{AppError, AppResult},
    guards::{lang::Language, perms::PermsEvaluator, user::User},
    models::{ActionKind, AffiliatedTagAssignment, Tag, TagMorphology, TargetKind},
    perms::{HivePermission, SystemsScope},
    resolver::IdentityResolver,
};

pub async fn get_one<'x, X>(system_id: &str, tag_id: &str, db: X) -> AppResult<Option<Tag>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let tag = sqlx::query_as(
        "SELECT *
            FROM tags
            WHERE system_id = $1 AND tag_id = $2",
    )
    .bind(system_id)
    .bind(tag_id)
    .fetch_optional(db)
    .await?;

    Ok(tag)
}

pub async fn require_one<'x, X>(system_id: &str, tag_id: &str, db: X) -> AppResult<Tag>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    get_one(system_id, tag_id, db)
        .await?
        .ok_or_else(|| AppError::NoSuchTag(system_id.to_owned(), tag_id.to_owned()))
}

pub async fn list_for_system<'x, X>(system_id: &str, db: X) -> AppResult<Vec<Tag>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let tags = sqlx::query_as(
        "SELECT *
            FROM tags
            WHERE system_id = $1
            ORDER BY tag_id",
    )
    .bind(system_id)
    .fetch_all(db)
    .await?;

    Ok(tags)
}

pub async fn list_group_assignments<'x, X>(
    system_id: &str,
    tag_id: &str,
    label_lang: Option<&Language>,
    username: Option<&str>,
    db: X,
    perms: Option<&PermsEvaluator>,
) -> AppResult<Vec<AffiliatedTagAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let today = Local::now().date_naive();
    let mut query = sqlx::QueryBuilder::new("SELECT ta.*");

    match label_lang {
        Some(Language::Swedish) => {
            query.push(", gs.name_sv AS label");
        }
        Some(Language::English) => {
            query.push(", gs.name_en AS label");
        }
        None => {}
    }

    query.push(" FROM all_tag_assignments ta");

    if label_lang.is_some() {
        query.push(
            " JOIN groups gs
                ON gs.id = ta.group_id
                AND gs.domain = ta.group_domain",
        );
    }

    if let Some(username) = username {
        // filter for specific user
        query.push(" JOIN all_groups_of(");
        query.push_bind(username);
        query.push(", ");
        query.push_bind(today);
        query.push(
            ") ag
                ON ag.id = ta.group_id
                AND ag.domain = ta.group_domain",
        );
    }

    query.push(" WHERE ta.system_id = ");
    query.push_bind(system_id);
    query.push(" AND ta.tag_id = ");
    query.push_bind(tag_id);
    query.push(" AND ta.group_id IS NOT NULL AND ta.group_domain IS NOT NULL");

    query.push(" ORDER BY (ta.id IS NULL)");
    if label_lang.is_some() {
        query.push(", label");
    }
    query.push(", ta.group_domain, ta.group_id");

    let mut assignments: Vec<AffiliatedTagAssignment> =
        query.build_query_as().fetch_all(db).await?;

    if let Some(perms) = perms {
        for assignment in &mut assignments {
            let min = HivePermission::AssignTags(SystemsScope::Id(assignment.system_id.clone()));
            // query should be OK since perms are cached by perm_id
            assignment.can_manage = Some(perms.satisfies(min).await?);
        }
    }

    Ok(assignments)
}

pub async fn list_user_assignments<'x, X>(
    system_id: &str,
    tag_id: &str,
    db: X,
    resolver: &Option<IdentityResolver>,
    perms: Option<&PermsEvaluator>,
) -> AppResult<Vec<AffiliatedTagAssignment>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let mut query = sqlx::QueryBuilder::new("SELECT *");

    query.push(
        " FROM all_tag_assignments
        WHERE system_id = ",
    );
    query.push_bind(system_id);
    query.push(" AND tag_id = ");
    query.push_bind(tag_id);
    query.push(" AND username IS NOT NULL");

    query.push(" ORDER BY (id IS NULL), username");

    let mut assignments: Vec<AffiliatedTagAssignment> =
        query.build_query_as().fetch_all(db).await?;

    if let Some(perms) = perms {
        for assignment in &mut assignments {
            let min = HivePermission::AssignTags(SystemsScope::Id(assignment.system_id.clone()));
            // query should be OK since perms are cached by perm_id
            assignment.can_manage = Some(perms.satisfies(min).await?);
        }
    }

    if let Some(resolver) = resolver {
        resolver
            .populate_identities(
                &mut assignments,
                |assignment| assignment.username.as_deref().unwrap(),
                |assignment, name| assignment.label = Some(name),
            )
            .await?;

        // need to re-sort by label
        assignments.sort_unstable_by_key(|assignment| {
            (
                assignment.id.is_none(),    // false comes first (direct assignment)
                assignment.label.is_none(), // false comes first (known label)
                assignment.label.clone(),
                assignment.username.clone(),
            )
        });
    }

    Ok(assignments)
}

pub async fn create_new<'v, 'x, X>(
    system_id: &str,
    dto: &CreateTagDto<'v>,
    db: X,
    user: &User,
) -> AppResult<Tag>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    // managing HIVE_SYSTEM_ID tags is not a self-preservation error because
    // these are necessary for $hive:manage-groups:tag == #hive:tag

    let mut txn = db.begin().await?;

    let tag: Tag = sqlx::query_as(
        "INSERT INTO tags
            (system_id, tag_id, supports_groups, supports_users, has_content, description)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING *",
    )
    .bind(system_id)
    .bind(dto.id)
    .bind(dto.supports_groups)
    .bind(dto.supports_users)
    .bind(dto.has_content)
    .bind(dto.description)
    .fetch_one(&mut *txn)
    .await
    .map_err(|e| AppError::DuplicateTagId(dto.id.to_string()).if_unique_violation(e))?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::Tag,
        tag.key(),
        user.username(),
        json!({
            "new": {
                "supports_groups": dto.supports_groups,
                "supports_users": dto.supports_users,
                "has_content": dto.has_content,
                "description": dto.description,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(tag)
}

pub async fn delete<'x, X>(system_id: &str, tag_id: &str, db: X, user: &User) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    // managing HIVE_SYSTEM_ID tags is not a self-preservation error because
    // these are necessary for $hive:manage-groups:tag == #hive:tag

    let mut txn = db.begin().await?;

    let old: Tag = sqlx::query_as(
        "DELETE FROM tags
        WHERE system_id = $1
            AND tag_id = $2
        RETURNING *",
    )
    .bind(system_id)
    .bind(tag_id)
    .fetch_optional(&mut *txn)
    .await?
    .ok_or_else(|| AppError::NoSuchTag(system_id.to_owned(), tag_id.to_owned()))?;

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::Tag,
        old.key(),
        user.username(),
        json!({
            "old": {
                "supports_groups": old.supports_groups,
                "supports_users": old.supports_users,
                "has_content": old.has_content,
                "description": old.description,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn assign_to_group<'v, 'x, X>(
    system_id: &str,
    tag_id: &str,
    dto: &AssignTagToGroupDto<'v>,
    label_lang: Option<&Language>,
    db: X,
    user: &User,
) -> AppResult<AffiliatedTagAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    assert_supported_assignment(system_id, tag_id, true, dto.content.is_some(), &mut *txn).await?;

    let mut query = sqlx::QueryBuilder::with_arguments(
        "INSERT INTO tag_assignments (system_id, tag_id, content, group_id, group_domain)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *, TRUE AS can_manage",
        pg_args!(
            system_id,
            tag_id,
            dto.content,
            dto.group.id,
            dto.group.domain
        ),
    );

    if let Some(lang) = label_lang {
        query.push(", (SELECT ");
        match lang {
            Language::Swedish => query.push("name_sv"),
            Language::English => query.push("name_en"),
        };
        query.push(
            " FROM groups gs
            WHERE gs.id = $4
                AND gs.domain = $5
            ) AS label",
        );
    }

    let mut assignment: AffiliatedTagAssignment = query
        .build_query_as()
        .fetch_one(&mut *txn)
        .await
        .map_err(|e| match e {
            sqlx::Error::Database(err) if err.is_unique_violation() => {
                AppError::DuplicateTagAssignment(
                    system_id.to_string(),
                    tag_id.to_string(),
                    dto.content.as_deref().map(ToString::to_string),
                )
            }
            sqlx::Error::Database(err) if err.is_foreign_key_violation() => {
                AppError::NoSuchGroup(dto.group.id.to_string(), dto.group.domain.to_string())
            }
            _ => e.into(),
        })?;

    assignment.can_manage = Some(true);

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::TagAssignment,
        assignment.key(),
        user.username(),
        json!({
            "new": {
                "entity_type": "group",
                "id": assignment.id,
                "group_id": assignment.group_id,
                "group_domain": assignment.group_domain,
                "content": assignment.content,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(assignment)
}

pub async fn assign_to_user<'v, 'x, X>(
    system_id: &str,
    tag_id: &str,
    dto: &AssignTagToUserDto<'v>,
    db: X,
    resolver: &Option<IdentityResolver>,
    user: &User,
) -> AppResult<AffiliatedTagAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    assert_supported_assignment(system_id, tag_id, false, dto.content.is_some(), &mut *txn).await?;

    let mut query = sqlx::QueryBuilder::with_arguments(
        "INSERT INTO tag_assignments (system_id, tag_id, content, username)
        VALUES ($1, $2, $3, $4)
        RETURNING *, TRUE AS can_manage",
        pg_args!(system_id, tag_id, dto.content, dto.user),
    );

    let mut assignment: AffiliatedTagAssignment = query
        .build_query_as()
        .fetch_one(&mut *txn)
        .await
        .map_err(|e| {
            AppError::DuplicateTagAssignment(
                system_id.to_string(),
                tag_id.to_string(),
                dto.content.as_deref().map(ToString::to_string),
            )
            .if_unique_violation(e)
        })?;

    assignment.can_manage = Some(true);

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::TagAssignment,
        assignment.key(),
        user.username(),
        json!({
            "new": {
                "entity_type": "user",
                "id": assignment.id,
                "username": assignment.username,
                "content": assignment.content,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    // design choice: a name resolution fail does not abort the transaction,
    // which (arguably) might make sense to allow management even when the
    // resolver is down / broken. this also means invalid usernames can still be
    // assigned tags, even if they don't exist
    // (see also: commit fdf8908115b3726c8b8bc119b1b24fb25a2d9900)
    if let Some(resolver) = resolver {
        assignment.label = resolver
            .resolve_one(assignment.username.as_deref().unwrap())
            .await?;
    }

    Ok(assignment)
}

pub async fn unassign<'x, X>(
    assignment_id: Uuid,
    db: X,
    perms: &PermsEvaluator,
    user: &User,
) -> AppResult<AffiliatedTagAssignment>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    let old: AffiliatedTagAssignment = sqlx::query_as(
        "DELETE FROM tag_assignments
        WHERE id = $1
        RETURNING *",
    )
    .bind(assignment_id)
    .fetch_optional(&mut *txn)
    .await?
    .ok_or_else(|| AppError::NotAllowed(HivePermission::AssignTags(SystemsScope::Wildcard)))?;
    // ^ not a permissions problem, but prevents enumeration (we haven't checked
    // permissions yet)

    let min = HivePermission::AssignTags(SystemsScope::Id(old.system_id.clone()));
    perms.require(min).await?;

    let details = if let Some(ref username) = old.username {
        json!({
            "old": {
                "entity_type": "user",
                "id": assignment_id,
                "username": username,
                "content": old.content,
            }
        })
    } else {
        let group_id = old.group_id.as_ref().expect("group id");
        let group_domain = old.group_domain.as_ref().expect("group domain");

        json!({
            "old": {
                "entity_type": "group",
                "id": assignment_id,
                "group_id": group_id,
                "group_domain": group_domain,
                "content": old.content,
            }
        })
    };

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::TagAssignment,
        // FIXME: consider using assignment_id as target_id
        old.key(),
        user.username(),
        details,
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(old)
}

pub async fn list_subtags<'v, 'x, X>(system_id: &str, tag_id: &str, db: X) -> AppResult<Vec<Tag>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let subtags: Vec<Tag> = sqlx::query_as(
        "SELECT ts.*
        FROM subtags st
        JOIN tags ts
            ON ts.tag_id = st.child_id
            AND ts.system_id = st.child_system_id
        WHERE parent_id = $1
            AND parent_system_id = $2",
    )
    .bind(tag_id)
    .bind(system_id)
    .fetch_all(db)
    .await?;

    Ok(subtags)
}

pub async fn create_subtag<'v, 'x, X>(
    system_id: &str,
    tag_id: &str,
    dto: &CreateSubtagDto<'v>,
    db: X,
    user: &User,
) -> AppResult<Tag>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    // note: this doesn't check whether the subtag has minimally compatible
    // supports_users/groups with the parent, but it's not really that big
    // of a problem anyway and shouldn't lead to logic errors

    let mut txn = db.begin().await?;

    let loop_detected = sqlx::query_scalar(
        "SELECT COUNT(*) > 0
        FROM tag_ancestry
        WHERE descendant_id = $1
            AND descendant_system_id = $2
            AND ancestor_id = $3
            AND ancestor_system_id = $4",
    )
    .bind(tag_id)
    .bind(system_id)
    .bind(dto.subtag.tag_id)
    .bind(dto.subtag.system_id)
    .fetch_one(&mut *txn)
    .await?;

    if loop_detected {
        // since tag_ancestry includes (x, x) nodes, this will automatically
        // catch trying to add a tag as a subtag of itself
        return Err(AppError::InvalidSubtag(
            dto.subtag.system_id.to_owned(),
            dto.subtag.tag_id.to_owned(),
        ));
    }

    sqlx::query(
        "INSERT INTO subtags
            (parent_id, parent_system_id, child_id, child_system_id)
        VALUES ($1, $2, $3, $4)",
    )
    .bind(tag_id)
    .bind(system_id)
    .bind(dto.subtag.tag_id)
    .bind(dto.subtag.system_id)
    .execute(&mut *txn)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(err) if err.is_unique_violation() => AppError::DuplicateSubtag(
            dto.subtag.system_id.to_owned(),
            dto.subtag.tag_id.to_owned(),
        ),
        sqlx::Error::Database(err) if err.is_foreign_key_violation() => AppError::NoSuchTag(
            dto.subtag.system_id.to_owned(),
            dto.subtag.tag_id.to_owned(),
        ),
        _ => e.into(),
    })?;

    let subtag: Tag = sqlx::query_as(
        "SELECT *
        FROM tags
        WHERE tag_id = $1
            AND system_id = $2",
    )
    .bind(dto.subtag.tag_id)
    .bind(dto.subtag.system_id)
    .fetch_one(&mut *txn)
    .await?;

    audit_logs::add_entry(
        ActionKind::Create,
        TargetKind::TagAssignment, // FIXME: consider independent Subtag target
        format!("#{system_id}:{tag_id}"),
        user.username(),
        json!({
            "new": {
                "entity_type": "subtag",
                "subtag_system_id": subtag.system_id,
                "subtag_tag_id": subtag.tag_id,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(subtag)
}

pub async fn unlink_subtag<'x, X>(
    system_id: &str,
    tag_id: &str,
    subtag_system_id: &str,
    subtag_tag_id: &str,
    db: X,
    user: &User,
) -> AppResult<()>
where
    X: sqlx::Acquire<'x, Database = sqlx::Postgres>,
{
    let mut txn = db.begin().await?;

    sqlx::query(
        "DELETE FROM subtags
        WHERE parent_id = $1
            AND parent_system_id = $2
            AND child_id = $3
            AND child_system_id = $4",
    )
    .bind(tag_id)
    .bind(system_id)
    .bind(subtag_tag_id)
    .bind(subtag_system_id)
    .execute(&mut *txn)
    .await?;

    audit_logs::add_entry(
        ActionKind::Delete,
        TargetKind::TagAssignment, // FIXME: consider independent Subtag target
        format!("#{system_id}:{tag_id}"),
        user.username(),
        json!({
            "old": {
                "entity_type": "subtag",
                "subtag_system_id": subtag_system_id,
                "subtag_tag_id": subtag_tag_id,
            }
        }),
        &mut *txn,
    )
    .await?;

    txn.commit().await?;

    Ok(())
}

pub async fn get_morphology<'x, X>(system_id: &str, tag_id: &str, db: X) -> AppResult<TagMorphology>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    sqlx::query_as(
        "SELECT has_content, supports_groups, supports_users
        FROM tags
        WHERE system_id = $1
            AND tag_id = $2",
    )
    .bind(system_id)
    .bind(tag_id)
    .fetch_optional(db)
    .await?
    .ok_or_else(|| AppError::NoSuchTag(system_id.to_string(), tag_id.to_string()))
}

pub async fn assert_supported_assignment<'x, X>(
    system_id: &str,
    tag_id: &str,
    assignment_to_group: bool,
    assignment_with_content: bool,
    db: X,
) -> AppResult<()>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let morph = get_morphology(system_id, tag_id, db).await?;

    let supports_entity = if assignment_to_group {
        morph.supports_groups
    } else {
        morph.supports_users
    };

    if !supports_entity {
        Err(AppError::UnsupportedTagAssignment(
            system_id.to_string(),
            tag_id.to_string(),
        ))
    } else if morph.has_content && !assignment_with_content {
        Err(AppError::MissingTagContent(
            system_id.to_string(),
            tag_id.to_string(),
        ))
    } else if !morph.has_content && assignment_with_content {
        Err(AppError::ExtraneousTagContent(
            system_id.to_string(),
            tag_id.to_string(),
        ))
    } else {
        Ok(())
    }
}
