use std::sync::{Arc, LazyLock};

use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    errors::{AppError, AppResult},
    integrations::grafana::grafana_labs::{GrafanaApiClient, NewTeam, UpdateTeamMembers},
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
                r#type: super::SettingType::Select(&[
                    super::SelectSettingOption {
                        value: "dry-run",
                        display_name: "Dry run",
                    },
                    super::SelectSettingOption {
                        value: "full",
                        display_name: "Complete push from Hive to Grafana",
                    },
                ]),
            },
            super::Setting {
                id: "service-account-key",
                secret: true,
                name: "Service Account Private Key",
                description: "Service account API-token",
                r#type: super::SettingType::ShortText,
            },
        ],
        tags: &[super::Tag {
            id: "member",
            description: "Entity whoes member should be sync'd to Grafana",
            has_content: true,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        }],
        tasks: &[super::Task {
            id: "sync-to-grafana",
            schedule: "0 0 * * * *", // every hour
            func: |mon, settings, resolver, db| {
                Box::pin(sync_to_grafana(mon, settings, resolver, db))
            },
        }],
    }
});

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    DryRun, // no actions are taken
    Full,   // complete push from Hive to Google directory
}

impl Mode {
    fn informational_message(&self) -> &'static str {
        match self {
            Self::DryRun => "Dry run is enabled. No actual changes will be made!",
            Self::Full => "Full push mode is selected: all reported changes are real!",
        }
    }

    fn should_insert(&self) -> bool {
        matches!(self, Self::Full)
    }

    fn should_update(&self) -> bool {
        matches!(self, Self::Full)
    }

    fn should_delete(&self) -> bool {
        matches!(self, Self::Full)
    }
}

macro_rules! fallible {
    ($mon:expr, $result:expr, $ret:expr) => {
        match $result {
            Ok(x) => x,
            Err(e) => {
                $mon.error(e);

                return Ok($ret);
            }
        }
    };
    ($mon:expr, $result:expr) => {
        fallible!($mon, $result, ())
    };
}

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

    let listed = fallible!(mon, client.list_teams().await).teams;

    for existing in &listed {
        if teams
            .binary_search_by_key(&existing.name, |t| t.to_string())
            .is_err()
        {
            mon.info(format!("Deleting team `{}`", existing.name));

            if mode.should_delete() {
                fallible!(mon, client.delete_team(existing.id).await);
            }
        }
    }

    let mut org_members: Vec<String> = fallible!(mon, client.list_org_members().await)
        .into_iter()
        .map(|member| member.email)
        .collect();

    org_members.sort_unstable();

    for team in &teams {
        mon.info(format!("Synchronizing team `{team}`"));

        let grafana_team_id = listed
            .iter()
            .find(|t| t.name == *team)
            .and_then(|g| Some(g.id));

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
        .bind(team)
        .fetch_one(&db)
        .await?;

        let group_members: Vec<models::GroupMember> =
            groups::members::get_all_members(&id, &domain, &db, None).await?;

        let usernames = group_members.iter().map(|member| member.username.as_str());

        let emails = if let Some(resolver) = resolver.as_ref() {
            resolver.resolve_emails(usernames.into_iter()).await?
        } else {
            return Err(AppError::ErrorDecodeFailure);
        };

        let members: Vec<String> = emails
            .into_iter()
            .map(|(_, email)| email)
            .filter(|member| org_members.binary_search_by_key(&member, |m| m).is_ok())
            .collect();

        // Because we need grafanas internal teamId to sync members, if the team doesn't
        // exist we first need to create the team and then sync the members
        if let Some(team_id) = grafana_team_id {
            sync_team_members(&team, team_id, members, &client, mode, mon).await?;
        } else {
            create_team(&team, members, &client, mode, mon).await?;
        }
    }

    mon.info(format!("Synchronized {} teams!", teams.len()));

    mon.succeeded();

    Ok(())
}

async fn create_team(
    key: &str,
    members: Vec<String>,
    client: &GrafanaApiClient,
    mode: Mode,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    mon.info(format!("Creating team: `{key}`"));

    if mode.should_insert() {
        let new = NewTeam {
            name: key.to_string(),
            email: String::new(), // Primarly used for gravatar which we don't use
        };

        let new_team = fallible!(mon, client.create_team(new).await);

        sync_team_members(key, new_team.team_id, members, client, mode, mon).await?;
    }

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
        if members.binary_search_by_key(&member, |m| m).is_err() {
            mon.info(format!("Removing member `{}` from team `{}`", member, key));
        }
    }

    for member in &members {
        if current_members
            .binary_search_by_key(&member, |m| m)
            .is_err()
        {
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
