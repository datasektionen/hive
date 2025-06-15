use std::sync::LazyLock;

use sqlx::PgPool;

use crate::{
    errors::AppResult, integrations::gworkspace::google::DirectoryApiClient, models,
    services::groups,
};

mod google;

// can't use const because it wouldn't support async fn pointers for tasks
pub static MANIFEST: LazyLock<super::Manifest> = LazyLock::new(|| super::Manifest {
    id: "gworkspace",
    description: "Sync users and groups to Google Workspace",
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
                    value: "push",
                    display_name: "Push from Hive",
                },
            ]),
        },
        super::Setting {
            id: "primary-domain",
            secret: false,
            name: "Primary Domain",
            description: "Where user accounts will be looked up & created",
            r#type: super::SettingType::ShortText,
        },
        super::Setting {
            id: "service-account-email",
            secret: false,
            name: "Service Account Email",
            description: "Google Cloud service account with 'Service Account Token Creator' role",
            r#type: super::SettingType::ShortText,
        },
        super::Setting {
            id: "service-account-key",
            secret: true,
            name: "Service Account Private Key",
            description: "Service account PEM-formatted private key (with header & footer)",
            r#type: super::SettingType::LongText,
        },
        super::Setting {
            id: "impersonate-user",
            secret: false,
            name: "Impersonate User",
            description: "Email address of the domain admin to impersonate",
            r#type: super::SettingType::ShortText,
        },
    ],
    tags: &[
        super::Tag {
            id: "sync",
            description: "Entity that should be sync'd to Google Workspace",
            has_content: false,
            supports_groups: true,
            supports_users: true,
        },
        super::Tag {
            id: "allow-external",
            description: "Allow non-Workspace users to be added to the group",
            has_content: false,
            supports_groups: true,
            supports_users: false,
        },
        super::Tag {
            id: "extra-member",
            description: "Additional email address to be added to the group",
            has_content: true,
            supports_groups: true,
            supports_users: false,
        },
        super::Tag {
            id: "personal-email",
            description: "Personal email address to be used when no Workspace user is found",
            has_content: true,
            supports_groups: false,
            supports_users: true,
        },
    ],
    tasks: &[super::Task {
        id: "sync-to-directory",
        schedule: "0 50 18 * * *",
        func: |mon, settings, db| Box::pin(sync_to_directory(mon, settings, db)),
    }],
});

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

async fn sync_to_directory(
    mon: &mut super::TaskRunMonitor,
    settings: super::SettingsValues,
    db: PgPool,
) -> AppResult<()> {
    let mode = super::require_string_setting!(mon, settings, "mode");
    let dry_run = mode != "push";

    let primary_domain = super::require_string_setting!(mon, settings, "primary-domain", '.');

    let service_account_email =
        super::require_string_setting!(mon, settings, "service-account-email", '@');
    let private_key = super::require_string_setting!(
        mon,
        settings,
        "service-account-key",
        "-----BEGIN PRIVATE KEY-----"
    );
    let impersonate_user = super::require_string_setting!(mon, settings, "impersonate-user", '@');

    let client = fallible!(
        mon,
        google::DirectoryApiClient::new(service_account_email, private_key, impersonate_user).await
    );

    if dry_run {
        mon.warn("Dry run is enabled. No actual changes will be made!")
    } else {
        mon.warn("Push mode is selected: all reported changes are real!")
    }

    // TODO: sync users first

    let groups: Vec<models::Group> = sqlx::query_as(
        "SELECT gs.*
        FROM all_tag_assignments ta
        JOIN groups gs
            ON gs.id = ta.group_id
                AND gs.domain = ta.group_domain
        WHERE ta.system_id = 'gworkspace'
            AND ta.tag_id = 'sync'
        ORDER BY gs.domain, gs.id",
    )
    .fetch_all(&db)
    .await?;

    // doing this before sync'ing groups to avoid listing newly-created;
    // means that we don't need to process groups that obviously should remain
    let listed = fallible!(mon, client.list_groups().await);
    for existing in listed {
        let (id, domain) = existing.email.split_once('@').expect("valid email");

        if groups
            .binary_search_by_key(&(domain, id), |g| (g.domain.as_str(), g.id.as_str()))
            .is_err()
        {
            mon.info(format!(
                "Deleting group <{}>: `{}`",
                existing.email, existing.name
            ));

            if !dry_run {
                todo!("delete group");
            }
        }
    }

    for group in &groups {
        let key = format!("{}@{}", group.id, group.domain);

        let allow_external = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM all_tag_assignments
                WHERE system_id = 'gworkspace'
                    AND tag_id = 'allow-external'
                    AND group_id = $1
                    AND group_domain = $2
            )",
        )
        .bind(&group.id)
        .bind(&group.domain)
        .fetch_one(&db)
        .await?;

        if allow_external {
            mon.info(format!("Group {key} allows external members"));
        }

        sync_group(&key, group, &client, dry_run, mon).await?;

        let subgroup_emails_owned: Vec<_> =
            groups::members::get_direct_subgroups(&group.id, &group.domain, &db)
                .await?
                .iter()
                .map(|s| s.group.key())
                .collect();

        let subgroup_emails: Vec<_> = subgroup_emails_owned.iter().map(String::as_str).collect();

        let direct_members_owned =
            groups::members::get_direct_members(&group.id, &group.domain, &db, &None).await?;

        let mut direct_members = vec![];

        for member in direct_members_owned {
            let with_email = get_user_email(
                &member.username,
                primary_domain,
                allow_external,
                &client,
                &db,
                mon,
            )
            .await?;

            if let Some(with_email) = with_email {
                direct_members.push(with_email);
            } else {
                mon.warn(format!(
                    "Skipping user {} (could not find suitable email)",
                    member.username
                ));
            }
        }

        sync_group_members(
            &key,
            &subgroup_emails,
            &direct_members,
            &client,
            dry_run,
            mon,
        )
        .await?;
    }

    mon.info(format!("Synchronized {} groups!", groups.len()));

    mon.succeeded();

    Ok(())
}

async fn sync_group(
    key: &str,
    group: &models::Group,
    client: &DirectoryApiClient,
    dry_run: bool,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    mon.info(format!("Synchronizing group `{key}`"));

    if let Some(current) = fallible!(mon, client.get_group(key).await) {
        // update existing

        let name_patch = if current.name != group.name_sv && current.name != group.name_en {
            mon.info(format!(
                "Updating name from `{}` to `{}`",
                current.name, group.name_sv
            ));

            Some(group.name_sv.as_str())
        } else {
            None
        };

        let mut truncated_description = group.description_sv.clone();
        truncated_description.truncate(4096); // max supported by Google Groups

        let desc_patch = if current.description != truncated_description
            && current.name != group.description_en
        {
            mon.info(format!(
                "Updating description from `{}` to `{}`",
                current.description, group.description_sv
            ));

            Some(group.description_sv.as_str())
        } else {
            None
        };

        if dry_run || (name_patch.is_none() && desc_patch.is_none()) {
            // nothing to do
            return Ok(());
        }

        let patch = google::GroupPatch {
            name: name_patch,
            description: desc_patch,
        };

        if fallible!(mon, client.patch_group(key, &patch).await).is_some() {
            mon.info(format!("Successfully updated group `{key}`"));

            // TODO: update group settings
            // https://developers.google.com/workspace/admin/groups-settings/v1/reference/groups
        } else {
            mon.warn(format!("Could not update group `{key}` (no longer exists)"));
        }
    } else {
        // create new group

        mon.error("not yet implemented: creating group");

        // truncate description at 4096 chars

        // TODO: update group settings
        // https://developers.google.com/workspace/admin/groups-settings/v1/reference/groups
    }

    Ok(())
}

async fn sync_group_members(
    key: &str,
    subgroup_emails: &[&str],
    direct_members: &[UserWithEmail],
    client: &DirectoryApiClient,
    dry_run: bool,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    let direct_member_emails: Vec<_> = direct_members.iter().map(|m| m.email.as_ref()).collect();

    let current = fallible!(mon, client.list_group_members(key).await);

    for entry in &current {
        let present = match entry.r#type {
            google::GroupMemberType::Group => subgroup_emails.contains(&entry.email.as_str()),
            google::GroupMemberType::User => direct_member_emails.contains(&entry.email.as_str()),
        };

        if !present {
            mon.info(format!(
                "Removing member `{}` from group `{}`",
                entry.email, key
            ));

            if !dry_run {
                fallible!(mon, client.remove_group_member(key, &entry.email).await);
            }
        }
    }

    let existing_emails: Vec<_> = current.iter().map(|m| m.email.as_str()).collect();

    for subgroup in subgroup_emails {
        // if already exists, nothing could be wrong
        // (Google already only supports Member role if it's a group)

        if !existing_emails.contains(subgroup) {
            mon.info(format!("Adding subgroup `{subgroup}` to group `{key}`"));

            if !dry_run {
                let member = google::GroupMember {
                    email: subgroup.to_string(),
                    role: google::GroupMemberRole::Member,
                    r#type: google::GroupMemberType::Group,
                    delivery_settings: Some(google::GroupMemberDeliverySettings::AllMail),
                };

                fallible!(mon, client.add_group_member(key, &member).await);
            }
        }
    }

    for direct_member in direct_members {
        let username = direct_member.username.as_str();

        if let Some(existing_member) = current.iter().find(|m| m.email == direct_member.email) {
            if existing_member.role != google::GroupMemberRole::Member {
                mon.info(format!("Demoting `{username}` to MEMBER in group `{key}`"));

                if !dry_run {
                    let patch = google::GroupMemberPatch {
                        role: google::GroupMemberRole::Member,
                    };

                    fallible!(
                        mon,
                        client
                            .patch_group_member(key, &direct_member.email, &patch)
                            .await
                    );
                }
            }
        } else {
            mon.info(format!("Adding member `{username}` to group `{key}`"));

            if !dry_run {
                let member = google::GroupMember {
                    email: direct_member.email.clone(),
                    role: google::GroupMemberRole::Member,
                    r#type: google::GroupMemberType::User,
                    delivery_settings: Some(google::GroupMemberDeliverySettings::AllMail),
                };

                fallible!(mon, client.add_group_member(key, &member).await);
            }
        }
    }

    Ok(())
}

async fn get_user_email(
    username: &str,
    primary_domain: &str,
    allow_external: bool,
    client: &DirectoryApiClient,
    db: &PgPool,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<Option<UserWithEmail>> {
    let lookup = format!("{username}@{primary_domain}");

    if let Some(user) = fallible!(mon, client.get_user(&lookup).await, None) {
        // user exists in domain!
        return Ok(Some(UserWithEmail {
            username: username.to_owned(),
            email: user.primary_email,
        }));
    }

    if !allow_external {
        mon.warn(format!("Cannot use a personal email for `{username}` because group does not support external members"));

        return Ok(None);
    }

    let personal = sqlx::query_scalar(
        "SELECT content
        FROM all_tag_assignments
        WHERE system_id = 'gworkspace'
            AND tag_id = 'personal-email'
            AND username = $1
            AND content LIKE '%@%.%'
        ORDER BY id
        LIMIT 1",
    )
    .bind(username)
    .fetch_optional(db)
    .await?;

    if let Some(email) = personal {
        mon.info(format!(
            "Using personal email `{email}` for user `{username}`"
        ));

        Ok(Some(UserWithEmail {
            username: username.to_owned(),
            email,
        }))
    } else {
        Ok(None)
    }
}

struct UserWithEmail {
    username: String,
    email: String,
}
