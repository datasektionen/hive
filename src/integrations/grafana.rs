use std::sync::{Arc, LazyLock};

use sqlx::PgPool;

use crate::{
    errors::{AppError, AppResult},
    integrations::{
        Mode, fallible,
        grafana::grafana_labs::{CreateTeam, GrafanaApiClient, UpdateTeamMembers},
    },
    models,
    resolver::IdentityResolver,
    services::groups,
};

mod grafana_labs;

// can't use const because it wouldn't support async fn pointers for tasks
pub static MANIFEST: LazyLock<super::Manifest> = LazyLock::new(|| {
    super::Manifest {
        id: "grafana",
        description: "Sync users to Grafana",
        settings: &[
            super::Setting {
                id: "mode",
                secret: false,
                name: "Mode",
                description: "Level of structural mirroring to enforce",
                r#type: super::SettingType::Select(super::MODE_OPTION),
            },
            super::Setting {
                id: "service-account-token",
                secret: true,
                name: "Service Account Token",
                description: "API-token for service account used when syncing",
                r#type: super::SettingType::ShortText,
            },
        ],
        permissions: &[
            super::Permission {
                id: "admin",
                has_scope: false,
                description: "Manage server-wide settings and access to resources",
            },
            super::Permission {
                id: "editor",
                has_scope: false,
                description: "Can view and edit dashboards, folders, and playlists",
            },
            super::Permission {
                id: "viewer",
                has_scope: false,
                description: "Can view dashboards, playlists, and query data sources",
            },
        ],
        tags: &[super::Tag {
            id: "member",
            description: "Entity whoes member should be sync'd to a team in Grafana",
            has_content: true,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        }],
        tasks: &[super::Task {
            id: "sync-to-grafana",
            schedule: "0 0 * * * *", // every day
            func: |mon, settings, resolver, db| {
                Box::pin(sync_to_grafana(mon, settings, resolver, db))
            },
        }],
    }
});

async fn sync_to_grafana(
    mon: &mut super::TaskRunMonitor,
    settings: super::SettingsValues,
    resolver: Arc<Option<IdentityResolver>>,
    db: PgPool,
) -> AppResult<()> {
    let mode: Mode = super::require_serde_setting!(mon, settings, "mode");

    let api_token = super::require_string_setting!(mon, settings, "service-account-key");

    let client = fallible!(mon, grafana_labs::GrafanaApiClient::new(api_token));

    mon.warn(mode.informational_message());

    let mut teams: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT content 
        FROM all_tag_assignments
        WHERE system_id = 'grafana'
            AND tag_id = 'member'
        ORDER BY content",
    )
    .fetch_all(&db)
    .await?;

    // must sort *again* despite already doing ORDER BY in postgres because
    // collation might be different, meaning that e.g. the dash in d-sys would
    // lead to it being placed by postgres in a different place than what rust
    // would expect, so the binary search below fails when it shouldn't
    teams.sort_unstable();

    let mut listed = fallible!(mon, client.list_teams().await).teams;

    listed.sort_unstable_by(|a, b| a.name.cmp(&b.name));

    for existing in &listed {
        if teams.binary_search(&existing.name).is_err() {
            mon.info(format!("Deleting team `{}`", existing.name));

            if mode.should_delete() {
                fallible!(mon, client.delete_team(existing.id).await);
            }
        }
    }

    for team in &teams {
        if listed
            .binary_search_by_key(&team.as_str(), |a| a.name.as_str())
            .is_err()
        {
            mon.info(format!("Creating team: `{team}`"));

            if mode.should_insert() {
                let new = CreateTeam {
                    name: team.to_owned(),
                    email: String::new(), // Primarly used for gravatar which we don't use
                };

                fallible!(mon, client.create_team(new).await);
            }
        }
    }

    // Get the updated list with the new teams because we need grafanas internal IDs
    let teams = fallible!(mon, client.list_teams().await).teams;

    let mut org_members: Vec<String> = fallible!(mon, client.list_org_members().await)
        .into_iter()
        .map(|member| member.email)
        .collect();

    org_members.sort_unstable();

    for team in &teams {
        mon.info(format!("Synchronizing team `{}`", team.name));

        let (id, domain): (String, String) = sqlx::query_as(
            "SELECT gs.id, gs.domain
            FROM all_tag_assignments ta
            JOIN groups gs
                ON gs.id = ta.group_id
                    AND gs.domain = ta.group_domain
            WHERE ta.system_id = 'grafana'
                AND ta.tag_id = 'member'
                AND ta.content = $1
            ORDER BY gs.domain, gs.id",
        )
        .bind(&team.name)
        .fetch_one(&db)
        .await?;

        let group_members: Vec<models::GroupMember> =
            groups::members::get_all_members(&id, &domain, &db, None).await?;

        let usernames = group_members.iter().map(|member| member.username.as_str());

        // Accounts in grafana are identified by their email which is assigned based on
        // what they have set in SSO
        let emails = resolver
            .as_ref()
            .as_ref()
            .ok_or(AppError::MissingIdentityResolver)?
            .resolve_emails(usernames.into_iter())
            .await?;

        // Only sync members who have an account in grafana
        let members: Vec<String> = emails
            .into_iter()
            .map(|(_, email)| email)
            .filter(|member| org_members.contains(member))
            .collect();

        sync_team_members(&team.name, team.id, members, &client, mode, mon).await?;
    }

    mon.info(format!("Synchronized {} teams!", teams.len()));

    mon.succeeded();

    Ok(())
}

async fn sync_team_members(
    key: &str,
    id: u32,
    members: Vec<String>,
    client: &GrafanaApiClient,
    mode: Mode,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    let mut current_members: Vec<String> = fallible!(mon, client.list_team_members(id).await)
        .into_iter()
        .map(|m| m.email)
        .collect();

    current_members.sort_unstable();

    for member in &current_members {
        if members.binary_search(&member).is_err() {
            mon.info(format!("Removing member `{}` from team `{}`", member, key));
        }
    }

    for member in &members {
        if current_members.binary_search(&member).is_err() {
            mon.info(format!("Adding member `{}` to team `{}`", member, key));
        }
    }

    if mode.should_update() {
        let update_team_members = UpdateTeamMembers {
            members,
            admins: Vec::new(), /* Since we administrate team using hive there is no need to have
                                 * admins in grafana */
        };

        fallible!(mon, client.sync_team_members(id, update_team_members).await);
    }

    Ok(())
}
