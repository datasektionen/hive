use std::{
    cmp::Ordering,
    collections::{hash_map::Entry, HashMap, HashSet},
};

use chrono::{Local, NaiveDate};
use rocket::futures::TryStreamExt;
use sqlx::{FromRow, Row};

use super::{GroupMembershipKind, RoleInGroup};
use crate::{
    errors::AppResult,
    guards::{lang::Language, perms::PermsEvaluator, user::User},
    models::{Group, GroupRef, SimpleGroup},
    perms::{GroupsScope, HivePermission, TagContent},
    sanitizers::SearchTerm,
    services::pg_args,
    HIVE_SYSTEM_ID,
};

pub struct GroupOverviewSummary {
    pub group: Group,
    pub membership_kind: Option<GroupMembershipKind>,
    pub role: Option<RoleInGroup>,
    pub n_direct_members: usize,
    pub n_total_members: usize,
    pub n_permissions: usize,
}

pub async fn list_summaries<'x, X>(
    q: Option<&str>,
    domain_filter: Option<&str>,
    db: X,
    perms: &PermsEvaluator,
    user: &User,
) -> AppResult<Vec<GroupOverviewSummary>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres> + Copy,
{
    let today = Local::now().date_naive();

    let mut summaries = HashMap::new();

    for entry in get_relevant_from_memberships(&today, q, domain_filter, db, user).await? {
        let stats = get_group_stats(&today, &entry.group.id, &entry.group.domain, db).await?;

        summaries.insert(
            (entry.group.id.clone(), entry.group.domain.clone()),
            GroupOverviewSummary {
                group: entry.group,
                membership_kind: Some(entry.membership_kind),
                role: Some(entry.role),
                n_permissions: stats.n_permissions,
                n_direct_members: stats.n_direct_members,
                n_total_members: stats.n_total_members,
            },
        );
    }

    for group in get_relevant_from_permissions(q, domain_filter, db, perms).await? {
        if let Entry::Vacant(entry) = summaries.entry((group.id.clone(), group.domain.clone())) {
            let stats = get_group_stats(&today, &group.id, &group.domain, db).await?;

            entry.insert(GroupOverviewSummary {
                group,
                membership_kind: None,
                role: None,
                n_permissions: stats.n_permissions,
                n_direct_members: stats.n_direct_members,
                n_total_members: stats.n_total_members,
            });
        }
    }

    Ok(summaries.into_values().collect())
}

struct GroupMembershipEntry {
    group: Group,
    membership_kind: GroupMembershipKind,
    role: RoleInGroup,
}

async fn get_relevant_from_memberships<'x, X>(
    today: &NaiveDate,
    q: Option<&str>,
    domain_filter: Option<&str>,
    db: X,
    user: &User,
) -> AppResult<Vec<GroupMembershipEntry>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres> + Copy,
{
    let mut query = sqlx::QueryBuilder::with_arguments(
        "SELECT *
        FROM all_groups_of($1, $2) ag
        JOIN groups gs
            ON ag.id = gs.id
            AND ag.domain = gs.domain",
        pg_args!(user.username(), today),
    );

    add_search_clauses(&mut query, q, Some("gs"), domain_filter.is_some());

    if let Some(domain) = domain_filter {
        query.push(" ag.domain = ");
        query.push_bind(domain);
    }

    query.push(" ORDER BY gs.id, gs.domain");

    let mut result = query.build().fetch(db);

    let mut entries = vec![];

    while let Some(row) = result.try_next().await? {
        let group = Group::from_row(&row)?;
        let path: Vec<GroupRef> = row.try_get("path")?;

        let (membership_kind, is_manager) = if let [.., subgroup, _] = &path[..] {
            let is_manager = sqlx::query_scalar(
                "SELECT manager
                FROM subgroups
                WHERE parent_id = $1
                    AND parent_domain = $2
                    AND child_id = $3
                    AND child_domain = $4",
            )
            .bind(&group.id)
            .bind(&group.domain)
            .bind(&subgroup.group_id)
            .bind(&subgroup.group_domain)
            .fetch_one(db)
            .await?;

            (GroupMembershipKind::Indirect, is_manager)
        } else {
            let is_manager = sqlx::query_scalar(
                "SELECT manager
                FROM direct_memberships
                WHERE username = $1
                    AND group_id = $2
                    AND group_domain = $3
                    AND $4 BETWEEN \"from\" AND until
                ORDER BY until DESC",
            )
            .bind(user.username())
            .bind(&group.id)
            .bind(&group.domain)
            .bind(today)
            .fetch_one(db)
            .await?;

            (GroupMembershipKind::Direct, is_manager)
        };

        let role = if is_manager {
            RoleInGroup::Manager
        } else {
            RoleInGroup::Member
        };

        entries.push(GroupMembershipEntry {
            group,
            membership_kind,
            role,
        });
    }

    Ok(entries)
}

async fn get_relevant_from_permissions<'x, X>(
    q: Option<&str>,
    domain_filter: Option<&str>,
    db: X,
    perms: &PermsEvaluator,
) -> AppResult<Vec<Group>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres> + Copy,
{
    let mut domains = HashSet::new();
    let mut tags = HashSet::new();

    let probes = [
        HivePermission::ManageGroups(GroupsScope::Any),
        HivePermission::ManageMembers(GroupsScope::Any),
    ];
    for probe in probes {
        if populate_from_permission(probe, &mut domains, &mut tags, domain_filter, perms).await? {
            // wildcard was found, just return everything
            return get_all_groups(q, domain_filter, db).await;
        };
    }

    let mut groups = vec![];

    if !domains.is_empty() {
        let mut query = sqlx::QueryBuilder::new("SELECT * FROM groups");
        add_search_clauses(&mut query, q, None, true);

        query.push(" domain = ANY(");
        query.push_bind(Vec::from_iter(domains));
        query.push(")");

        groups.extend(query.build_query_as().fetch_all(db).await?);
    }

    if !tags.is_empty() {
        let mut query = sqlx::QueryBuilder::new(
            "SELECT gs.*
            FROM groups gs
            JOIN all_tag_assignments ta
                ON gs.id = ta.group_id
                AND gs.domain = ta.group_domain",
        );
        add_search_clauses(&mut query, q, Some("gs"), true);

        if let Some(domain) = domain_filter {
            query.push(" gs.domain = ");
            query.push_bind(domain);
            query.push(" AND");
        }

        add_tag_clauses(&mut query, tags);

        groups.extend(query.build_query_as().fetch_all(db).await?);
    }

    Ok(groups)
}

// returns true if wildcard is found
pub(super) async fn populate_from_permission(
    probe: HivePermission,
    domains: &mut HashSet<String>,
    tags: &mut HashSet<(String, Option<TagContent>)>,
    domain_filter: Option<&str>,
    perms: &PermsEvaluator,
) -> AppResult<bool> {
    for perm in perms.fetch_all_related(probe).await? {
        let scope = match perm {
            HivePermission::ManageGroups(scope) => scope,
            HivePermission::ManageMembers(scope) => scope,
            _ => continue,
        };

        match scope {
            GroupsScope::Domain(domain) => match domain_filter {
                None => domains.insert(domain),
                Some(filter) if filter == domain => domains.insert(domain),
                _ => false,
            },
            GroupsScope::Tag { id, content } => tags.insert((id, content)),
            GroupsScope::Wildcard => return Ok(true),
            GroupsScope::Any => unreachable!("? is not a real scope"),
            GroupsScope::AnyDomain => unreachable!("?@ is not a real scope"),
        };
    }

    Ok(false)
}

async fn get_all_groups<'x, X>(
    q: Option<&str>,
    domain_filter: Option<&str>,
    db: X,
) -> AppResult<Vec<Group>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres>,
{
    let mut query = sqlx::QueryBuilder::new("SELECT * FROM groups");

    add_search_clauses(&mut query, q, None, domain_filter.is_some());

    if let Some(domain) = domain_filter {
        query.push(" domain = ");
        query.push_bind(domain);
    }

    Ok(query.build_query_as().fetch_all(db).await?)
}

fn add_search_clauses(
    query: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    q: Option<&str>,
    table_alias: Option<&str>,
    additional_conds: bool,
) {
    const SEARCH_COLS: &[&str] = &[
        "id",
        "domain",
        "name_sv",
        "name_en",
        "description_sv",
        "description_en",
    ];

    if let Some(search) = q {
        // this will push the same bind many times even though all could be
        // references to the same $1 param... there doesn't seem to be a
        // way to avoid this, since push_bind adds $n to the query itself.
        // maybe alternatively we could do one ILIKE for the concatenation
        // of all these fields? would be a bit messy though...
        let term = SearchTerm::from(search).anywhere();

        query.push(" WHERE (");

        for (i, col) in SEARCH_COLS.iter().enumerate() {
            if i > 0 {
                query.push(" OR ");
            }
            if let Some(alias) = table_alias {
                query.push(alias);
                query.push(".");
            }
            query.push(col);
            query.push(" ILIKE ");
            query.push_bind(term.clone());
        }

        query.push(")");

        if additional_conds {
            // prepare for more conditions
            query.push(" AND");
        }
    } else if additional_conds {
        // need to have WHERE anyway to prepare for more conditions
        query.push(" WHERE");
    }
}

pub(super) fn add_tag_clauses(
    query: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    tags: HashSet<(String, Option<TagContent>)>,
) {
    if tags.is_empty() {
        // nothing to do
        return;
    }

    query.push(" ta.system_id = ");
    query.push_bind(HIVE_SYSTEM_ID);
    query.push(" AND (");

    for (i, (id, content)) in tags.into_iter().enumerate() {
        if i > 0 {
            query.push(" OR ");
        }
        match content {
            None | Some(TagContent::Wildcard) => {
                query.push("ta.tag_id = ");
                query.push_bind(id);
            }
            Some(TagContent::Custom(content)) => {
                query.push("(ta.tag_id = ");
                query.push_bind(id);
                query.push(" AND ta.content = ");
                query.push_bind(content);
                query.push(")");
            }
        }
    }

    query.push(")");
}

struct GroupStatistics {
    n_permissions: usize,
    n_direct_members: usize,
    n_total_members: usize,
}

async fn get_group_stats<'x, X>(
    today: &NaiveDate,
    id: &str,
    domain: &str,
    db: X,
) -> AppResult<GroupStatistics>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres> + Copy,
{
    let members = sqlx::query(
        "SELECT
            COUNT(DISTINCT username) AS n_total_members,
            COUNT(DISTINCT
                CASE
                    WHEN ARRAY_LENGTH(path, 1) = 1 THEN username
                END
            ) AS n_direct_members
        FROM all_members_of($1, $2, $3)",
    )
    .bind(id)
    .bind(domain)
    .bind(today)
    .fetch_one(db)
    .await?;

    let n_direct_members = members
        .try_get::<i64, _>("n_direct_members")?
        .try_into()
        .unwrap_or(usize::MAX);
    let n_total_members = members
        .try_get::<i64, _>("n_total_members")?
        .try_into()
        .unwrap_or(usize::MAX);

    let n_permissions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
        FROM permission_assignments
        WHERE group_id = $1
            AND group_domain = $2",
    )
    .bind(id)
    .bind(domain)
    .fetch_one(db)
    .await?
    .try_into()
    .unwrap_or(usize::MAX);

    Ok(GroupStatistics {
        n_direct_members,
        n_total_members,
        n_permissions,
    })
}

pub async fn list_all_permissible<'x, X>(
    db: X,
    perms: &PermsEvaluator,
    user: &User,
) -> AppResult<HashSet<SimpleGroup>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres> + Copy,
{
    let today = Local::now().date_naive();

    let mut groups = HashSet::new();

    // from membership
    groups.extend(
        sqlx::query_as(
            "SELECT DISTINCT gs.id, gs.domain, gs.name_sv, gs.name_en
            FROM all_groups_of($1, $2) ag
            JOIN groups gs
                ON gs.id = ag.id
                AND gs.domain = ag.domain",
        )
        .bind(user.username())
        .bind(today)
        .fetch_all(db)
        .await?,
    );

    // from permissions
    let mut domains = HashSet::new();
    let mut tags = HashSet::new();

    let probes = [
        HivePermission::ManageGroups(GroupsScope::Any),
        HivePermission::ManageMembers(GroupsScope::Any),
    ];
    for probe in probes {
        if populate_from_permission(probe, &mut domains, &mut tags, None, perms).await? {
            // wildcard was found, just return everything
            let all = sqlx::query_as("SELECT id, domain, name_sv, name_en FROM groups")
                .fetch_all(db)
                .await?;

            return Ok(HashSet::from_iter(all));
        };
    }

    if !domains.is_empty() {
        groups.extend(
            sqlx::query_as(
                "SELECT id, domain, name_sv, name_en
                FROM groups
                WHERE domain = ANY($1)",
            )
            .bind(Vec::from_iter(domains))
            .fetch_all(db)
            .await?,
        );
    }

    if !tags.is_empty() {
        let mut query = sqlx::QueryBuilder::new(
            "SELECT gs.*
            FROM groups gs
            JOIN all_tag_assignments ta
                ON gs.id = ta.group_id
                AND gs.domain = ta.group_domain
            WHERE",
        );

        add_tag_clauses(&mut query, tags);

        groups.extend(query.build_query_as().fetch_all(db).await?);
    }

    Ok(groups)
}

pub async fn list_all_permissible_sorted<'x, X>(
    lang: &Language,
    db: X,
    perms: &PermsEvaluator,
    user: &User,
) -> AppResult<Vec<SimpleGroup>>
where
    X: sqlx::Executor<'x, Database = sqlx::Postgres> + Copy,
{
    let mut groups = Vec::from_iter(list_all_permissible(db, perms, user).await?);

    // using sort_by_key would be more concise but require cloning all fields...
    groups.sort_by(
        |a, b| match (a.localized_name(lang)).cmp(b.localized_name(lang)) {
            Ordering::Equal => match a.id.cmp(&b.id) {
                Ordering::Equal => a.domain.cmp(&b.domain),
                other => other,
            },
            other => other,
        },
    );

    Ok(groups)
}
