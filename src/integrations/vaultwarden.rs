use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use sqlx::PgPool;

use crate::{
    errors::AppResult,
    integrations::{
        Mode, fallible,
        vaultwarden::bitwarden::{GroupSync, InviteData},
    },
    resolver::IdentityResolver,
    services::{self, groups},
};

mod bitwarden;

// can't use const because it wouldn't support async fn pointers for tasks
pub static MANIFEST: LazyLock<super::Manifest> = LazyLock::new(|| {
    super::Manifest {
        id: "vaultwarden",
        description: "Sync users to Vaultwarden",
        settings: &[
            super::Setting {
                id: "bitwarden-cli-url",
                secret: false,
                name: "Bitwarden CLI URL",
                description: "Url to the bitwarden cli",
                r#type: super::SettingType::ShortText,
            },
            super::Setting {
                id: "client-id",
                secret: true,
                name: "Client Id",
                description: "Client id of service account",
                r#type: super::SettingType::ShortText,
            },
            super::Setting {
                id: "client-secret",
                secret: true,
                name: "Client Secret",
                description: "Client secret of service account",
                r#type: super::SettingType::ShortText,
            },
            super::Setting {
                id: "master-password",
                secret: true,
                name: "Master Password",
                description: "Master password of service account",
                r#type: super::SettingType::ShortText,
            },
            super::Setting {
                id: "mode",
                secret: false,
                name: "Mode",
                description: "Level of structural mirroring to enforce",
                r#type: super::SettingType::Select(super::MODE_OPTIONS),
            },
            super::Setting {
                id: "organisation-id",
                secret: false,
                name: "Organisation ID",
                description: "The vaultwarden organisation id",
                r#type: super::SettingType::ShortText,
            },
            super::Setting {
                id: "vaultwarden-url",
                secret: false,
                name: "Vaultwarden URL",
                description: "URL to the vaultwarden instance without any path",
                r#type: super::SettingType::ShortText,
            },
        ],
        permissions: &[],
        tags: &[
            super::Tag {
                id: "sync",
                description: "Entity that should be sync'd to Vaultwarden",
                has_content: false,
                supports_groups: true,
                supports_users: false,
                self_service: false,
            },
            super::Tag {
                id: "access",
                description: "User who wants access to vaultwarden",
                has_content: false,
                supports_groups: false,
                supports_users: true,
                self_service: true,
            },
        ],
        tasks: &[
            super::Task {
                id: "invite-members-to-vaultwarden",
                schedule: "0 0 * * * *", // every hour
                func: |mon, settings, resolver, db| {
                    Box::pin(invite_members(mon, settings, resolver, db))
                },
            },
            super::Task {
                id: "sync-groups-to-vaultwarden",
                schedule: "0 0 0,12 * * *", // twise a day
                func: |mon, settings, resolver, db| {
                    Box::pin(sync_groups(mon, settings, resolver, db))
                },
            },
        ],
    }
});

async fn invite_members(
    mon: &mut super::TaskRunMonitor,
    settings: super::SettingsValues,
    resolver: Arc<Option<IdentityResolver>>,
    db: PgPool,
) -> AppResult<()> {
    let mode: Mode = super::require_serde_setting!(mon, settings, "mode");

    let client_id = super::require_string_setting!(mon, settings, "client-id");
    let client_secret = super::require_string_setting!(mon, settings, "client-secret");

    let bitwarden_url = super::require_string_setting!(mon, settings, "bitwarden-cli-url");
    let vault_url = super::require_string_setting!(mon, settings, "vaultwarden-url");
    let org_id = super::require_string_setting!(mon, settings, "organisation-id");

    let master_password = super::require_string_setting!(mon, settings, "master-password");

    let org_client = fallible!(
        mon,
        bitwarden::OrganisationAPIClient::new(
            vault_url.to_string(),
            org_id.to_string(),
            client_id,
            client_secret
        )
        .await
    );

    let vault_client = fallible!(
        mon,
        bitwarden::VaultManagementAPIClient::new(
            bitwarden_url.to_string(),
            org_id.to_string(),
            master_password.clone()
        )
    );

    mon.warn(mode.informational_message());

    let vault_users: HashMap<String, bitwarden::User> =
        fallible!(mon, org_client.list_org_members().await)
            .data
            .into_iter()
            .map(|user| (user.email.clone(), user))
            .collect();

    let hive_groups = groups::tags::list_tagged_for_system("vaultwarden", &db).await?;

    let mut users = HashMap::new();

    // Get all users who should have access based on a group they are in
    for group in hive_groups {
        let members: HashMap<String, String> = groups::members::get_all_members(
            &group.id,
            &group.domain,
            &db,
            resolver.as_ref().as_ref(),
        )
        .await?
        .into_iter()
        .filter_map(|member| {
            if let Some(email) = member.email
                && vault_users.get(&email).is_none()
            {
                Some((email, member.username))
            } else {
                None
            }
        })
        .collect();

        users.extend(members);
    }

    // Get all users who have requested access
    let unaffiliated_users: Vec<_> =
        services::tags::list_user_assignments("vaultwarden", "access", &db, None, None)
            .await?
            .into_iter()
            .filter_map(|assignment| assignment.username)
            .collect();

    if let Some(resolver) = resolver.as_ref() {
        let emails: HashMap<String, String> = resolver
            .resolve_emails(unaffiliated_users.iter().map(|username| username.as_str()))
            .await?
            .into_iter()
            .filter_map(|(kthid, email)| {
                if vault_users.get(&email).is_none() {
                    Some((email, kthid))
                } else {
                    None
                }
            })
            .collect();

        users.extend(emails);
    }

    for (email, kthid) in users.iter() {
        mon.info(format!("Inviting user `{kthid}` with email `{email}`",));
    }

    let emails: Vec<_> = users.into_keys().collect();

    if mode.should_insert() {
        let body = InviteData {
            emails,
            groups: Vec::new(),
            r#type: 2, // User
            collections: None,
            permissions: HashMap::new(),
        };

        fallible!(mon, org_client.invite_users(body).await);
    }

    // There is no need to do the extra computation to a HashMap because vaultwarden guarantees that
    // there is only one user per email
    let users_to_confirm: Vec<_> = vault_users
        .into_iter()
        .filter_map(|(email, user)| {
            if let Some(id) = user.id
                && user.status == 1
            {
                Some((id, email))
            } else {
                None
            }
        })
        .collect();

    fallible!(mon, vault_client.unlock().await);

    for (id, email) in users_to_confirm {
        mon.info(format!("Confirming `{email}`"));

        if mode.should_update() {
            fallible!(mon, vault_client.confirm(&id).await);
        }
    }

    fallible!(mon, vault_client.lock().await);

    mon.succeeded();

    Ok(())
}

async fn sync_groups(
    mon: &mut super::TaskRunMonitor,
    settings: super::SettingsValues,
    resolver: Arc<Option<IdentityResolver>>,
    db: PgPool,
) -> AppResult<()> {
    let mode: Mode = super::require_serde_setting!(mon, settings, "mode");

    let client_id = super::require_string_setting!(mon, settings, "client-id");
    let client_secret = super::require_string_setting!(mon, settings, "client-secret");

    let base_url = super::require_string_setting!(mon, settings, "vaultwarden-url");
    let org_id = super::require_string_setting!(mon, settings, "organisation-id");

    let client = fallible!(
        mon,
        bitwarden::OrganisationAPIClient::new(
            base_url.to_string(),
            org_id.to_string(),
            client_id,
            client_secret
        )
        .await
    );

    mon.warn(mode.informational_message());

    let vault_users: HashMap<String, bitwarden::User> =
        fallible!(mon, client.list_org_members().await)
            .data
            .into_iter()
            .map(|user| (user.email.clone(), user))
            .collect();

    let mut hive_groups: Vec<_> = fallible!(
        mon,
        groups::tags::list_tagged_for_system("vaultwarden", &db).await
    )
    .into_iter()
    .map(|group| (group.id, group.domain))
    .collect();

    hive_groups.push((String::from("members"), String::from("datasektionen.se")));

    hive_groups.sort_unstable();

    let mut vault_groups: Vec<bitwarden::Group> = fallible!(mon, client.list_groups().await).data;

    vault_groups.sort_unstable_by(|a, b| Ord::cmp(&a.name, &b.name));

    for vault_group in &vault_groups {
        let is_hive_group = hive_groups
            .binary_search_by_key(&vault_group.name, |(id, domain)| {
                format!("{}@{}", id, domain)
            })
            .is_ok();

        if !is_hive_group {
            mon.info(format!("Deleting group `{}`", vault_group.name));

            if mode.should_delete() {
                fallible!(mon, client.delete_group(&vault_group.id).await);
            }
        }
    }

    for (id, domain) in &hive_groups {
        // Get the corresponding hive members in the vault organisation
        let hive_members: Vec<bitwarden::User> = if id == "members" && domain == "datasektionen.se"
        {
            vault_users.iter().map(|(_, user)| user.clone()).collect()
        } else {
            let group_members =
                groups::members::get_all_members(id, domain, &db, resolver.as_ref().as_ref())
                    .await?;

            // Only sync members who have an account in vaultwarden
            group_members
                .into_iter()
                .filter_map(|member| {
                    if let Some(email) = member.email {
                        vault_users.get(&email)
                    } else {
                        None
                    }
                })
                .cloned()
                .collect()
        };

        let vault_group = vault_groups
            .iter()
            .find(|existing| existing.name == format!("{id}@{domain}"));

        if let Some(vault_group) = vault_group {
            mon.info(format!("Synchronizing group `{id}@{domain}`"));

            let vault_members = vault_users
                .iter()
                .filter(|(_, user)| user.groups.contains(&vault_group.id));

            for (_, vault_user) in vault_members {
                let is_hive_member = hive_members.contains(vault_user);

                if !is_hive_member {
                    mon.info(format!(
                        "Removing member `{}` from group `{id}@{domain}`",
                        vault_user.email
                    ));
                }
            }

            for member in &hive_members {
                let is_vault_member = member.groups.contains(&vault_group.id);

                if !is_vault_member {
                    mon.info(format!(
                        "Adding member `{}` to group `{id}@{domain}`",
                        member.email
                    ));
                }
            }

            let users = hive_members
                .into_iter()
                .filter_map(|user| user.id)
                .collect();

            if mode.should_update() {
                let body = GroupSync {
                    name: format!("{id}@{domain}"),
                    access_all: false, // no group should have access to all passwords
                    external_id: None, // used when manageing with ldap
                    collections: vault_group.collections.clone(),
                    users,
                };

                fallible!(mon, client.update_group(&vault_group.id, body).await);
            }
        } else {
            mon.info(format!("Creating group: `{id}@{domain}`"));

            for member in &hive_members {
                mon.info(format!(
                    "Adding member `{}` to group `{id}@{domain}`",
                    member.email
                ));
            }

            let users = hive_members
                .into_iter()
                .filter_map(|user| user.id)
                .collect();

            if mode.should_insert() {
                let body = GroupSync {
                    name: format!("{id}@{domain}"),
                    access_all: false, // no group should have access to all passwords
                    external_id: None, // used when manageing with ldap
                    collections: Vec::new(),
                    users,
                };

                fallible!(mon, client.create_group(body).await);
            }
        }
    }

    mon.succeeded();

    Ok(())
}
