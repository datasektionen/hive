use std::{collections::HashSet, sync::LazyLock};

use serde::Deserialize;
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
                    value: "no-deletion",
                    display_name: "Sync without removing existing entities",
                },
                super::SelectSettingOption {
                    value: "full",
                    display_name: "Complete push from Hive to Google directory",
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
        super::Setting {
            id: "group-whitelist",
            secret: false,
            name: "Group Whitelist",
            description: "Comma-separated list of group email addresses to never delete",
            r#type: super::SettingType::LongText,
        },
    ],
    tags: &[
        super::Tag {
            id: "sync",
            description: "Entity that should be sync'd to Google Workspace",
            has_content: false,
            supports_groups: true,
            supports_users: true,
            self_service: false,
        },
        super::Tag {
            id: "allow-external",
            description: "Allow non-Workspace users to be added to the group",
            has_content: false,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        },
        super::Tag {
            id: "grace-period",
            description: "Keep old members until a month past their membership end date",
            has_content: false,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        },
        super::Tag {
            id: "extra-member",
            description: "Additional email address to be added to the group",
            has_content: true,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        },
        super::Tag {
            id: "extra-subgroup",
            description: "Additional Google-only subgroup email address",
            has_content: true,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        },
        super::Tag {
            id: "embed-members",
            // ^ this is generally unnecessary, but useful in cases where we
            // cannot use "extra-subgroup": for example, if on Google group A
            // should include the members of group B, but only those tracked by
            // Hive and not any additional "extra-member"s of group B. in this
            // case, we must express that group B's (Hive) members are embedded
            // in group A's Google Group mirror
            description: "Hive group from where to take additional Google-only members",
            has_content: true,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        },
        super::Tag {
            id: "personal-email",
            description: "Personal email address to be used when no Workspace user is found",
            has_content: true,
            supports_groups: false,
            supports_users: true,
            self_service: true,
        },
    ],
    tasks: &[super::Task {
        id: "sync-to-directory",
        schedule: "0 0 6,18 * * *",
        func: |mon, settings, db| Box::pin(sync_to_directory(mon, settings, db)),
    }],
});

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    DryRun,     // no actions are taken
    NoDeletion, // unwarranted groups and members are never removed
    Full,       // complete push from Hive to Google directory
}

impl Mode {
    fn informational_message(&self) -> &'static str {
        match self {
            Self::DryRun => "Dry run is enabled. No actual changes will be made!",
            Self::NoDeletion => "No deletion is enabled. Existing entities will be preserved!",
            Self::Full => "Full push mode is selected: all reported changes are real!",
        }
    }

    fn should_insert(&self) -> bool {
        matches!(self, Self::NoDeletion | Self::Full)
    }

    fn should_update(&self) -> bool {
        matches!(self, Self::NoDeletion | Self::Full)
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

async fn sync_to_directory(
    mon: &mut super::TaskRunMonitor,
    settings: super::SettingsValues,
    db: PgPool,
) -> AppResult<()> {
    let mode: Mode = super::require_serde_setting!(mon, settings, "mode");

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

    mon.warn(mode.informational_message());

    // TODO: sync users first

    let mut groups: Vec<models::Group> = sqlx::query_as(
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

    // must sort *again* despite already doing ORDER BY in postgres because
    // collation might be different, meaning that e.g. the dash in d-sys would
    // lead to it being placed by postgres in a different place than what rust
    // would expect, so the binary search below fails when it shouldn't
    groups.sort_unstable_by(|a, b| a.domain.cmp(&b.domain).then(a.id.cmp(&b.id)));

    let mut whitelist = if let Some(serde_json::Value::String(s)) = settings.get("group-whitelist")
    {
        s.split(',').filter(|e| e.contains('@')).collect()
    } else {
        Vec::new()
    };
    whitelist.sort_unstable();

    // doing this before sync'ing groups to avoid listing newly-created;
    // means that we don't need to process groups that obviously should remain
    let listed = fallible!(mon, client.list_groups().await);

    for existing in &listed {
        let (id, domain) = existing.email.split_once('@').expect("valid email");

        if groups
            .binary_search_by_key(&(domain, id), |g| (g.domain.as_str(), g.id.as_str()))
            .is_err()
        {
            if whitelist.binary_search(&existing.email.as_str()).is_ok() {
                mon.info(format!(
                    "Not deleting whitelisted group `{}`",
                    existing.email
                ));

                continue;
            }

            mon.info(format!(
                "Deleting group <{}>: `{}` --- {:?}",
                existing.email, existing.name, existing
            ));

            let members = fallible!(mon, client.list_group_members(&existing.email).await);
            mon.info(format!(
                "Group `{}` had members: {:?}",
                existing.email, members
            ));

            if mode.should_delete() {
                fallible!(mon, client.delete_group(&existing.email).await);
            }
        }
    }

    let mut existing_emails: Vec<_> = listed
        .iter()
        .map(|existing| existing.email.to_lowercase())
        .collect();
    existing_emails.sort_unstable(); // to allow binary search

    for group in &groups {
        let key = format!("{}@{}", group.id, group.domain);

        mon.info(format!("Synchronizing group `{key}`"));

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

        if existing_emails.binary_search(&key).is_err() {
            // this group wasn't in the listing, so we need to create it
            create_group(&key, group, &client, mode, mon).await?;
        }

        sync_group_settings(&key, group, &client, mode, mon).await?;

        let subgroup_emails_owned: Vec<_> =
            groups::members::get_direct_subgroups(&group.id, &group.domain, &db)
                .await?
                .iter()
                .map(|s| s.group.key().to_lowercase())
                .collect();

        let extra_subgroups: Vec<String> = sqlx::query_scalar(
            "SELECT LOWER(content)
            FROM all_tag_assignments
            WHERE system_id = 'gworkspace'
                AND tag_id = 'extra-subgroup'
                AND group_id = $1
                AND group_domain = $2
                AND content LIKE '%@%.%'",
        )
        .bind(&group.id)
        .bind(&group.domain)
        .fetch_all(&db)
        .await?;

        let subgroup_emails: Vec<_> = subgroup_emails_owned
            .iter()
            .chain(extra_subgroups.iter())
            .map(String::as_str)
            .collect();

        let has_grace_period = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                FROM all_tag_assignments
                WHERE system_id = 'gworkspace'
                    AND tag_id = 'grace-period'
                    AND group_id = $1
                    AND group_domain = $2
            )",
        )
        .bind(&group.id)
        .bind(&group.domain)
        .fetch_one(&db)
        .await?;

        let grace_period = if has_grace_period {
            // 2025-02-01 becomes 2025-03-01, etc.
            Some(chrono::Months::new(1))
        } else {
            None
        };

        let mut direct_members_owned = groups::members::get_direct_members(
            &group.id,
            &group.domain,
            false,
            grace_period,
            &db,
            None,
        )
        .await?;

        let embeddings: Vec<String> = sqlx::query_scalar(
            "SELECT LOWER(content)
                    FROM all_tag_assignments
                    WHERE system_id = 'gworkspace'
                        AND tag_id = 'embed-members'
                        AND group_id = $1
                        AND group_domain = $2
                        AND content LIKE '%@%.%'",
        )
        .bind(&group.id)
        .bind(&group.domain)
        .fetch_all(&db)
        .await?;

        for embedding in embeddings {
            if let Some((id, domain)) = embedding.split_once('@') {
                let embedded = groups::members::get_all_members(id, domain, &db, None).await?;

                direct_members_owned.extend(embedded)
            }
        }

        let mut direct_members = HashSet::new();

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
                direct_members.insert(with_email);
            } else {
                mon.warn(format!(
                    "Skipping user {} (could not find suitable email)",
                    member.username
                ));
            }
        }

        let extra_members: Vec<UserWithEmail> = sqlx::query_scalar(
            "SELECT LOWER(content)
            FROM all_tag_assignments
            WHERE system_id = 'gworkspace'
                AND tag_id = 'extra-member'
                AND group_id = $1
                AND group_domain = $2
                AND content LIKE '%@%.%'",
        )
        .bind(&group.id)
        .bind(&group.domain)
        .fetch_all(&db)
        .await?
        .into_iter()
        .filter_map(UserWithEmail::new_extra)
        .collect();

        direct_members.extend(extra_members);

        sync_group_members(&key, &subgroup_emails, &direct_members, &client, mode, mon).await?;
    }

    mon.info(format!("Synchronized {} groups!", groups.len()));

    mon.succeeded();

    Ok(())
}

async fn create_group(
    key: &str,
    group: &models::Group,
    client: &DirectoryApiClient,
    mode: Mode,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    mon.info(format!("Creating group `{key}`"));

    if mode.should_insert() {
        let mut truncated_description = group.description_sv.clone();
        truncated_description.truncate(4096); // max supported by Google Groups

        let new = google::NewGroup {
            email: key.to_owned(),
            name: group.name_sv.clone(),
            description: truncated_description,
        };

        fallible!(mon, client.create_group(&new).await);

        mon.warn(format!(
            "Successfully created group `{key}`, but it will likely remain empty since Google API \
             will refuse to acknowledge it for a few minutes until stabilizing"
        ));
    }

    Ok(())
}

async fn sync_group_settings(
    key: &str,
    group: &models::Group,
    client: &DirectoryApiClient,
    mode: Mode,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    let Some(current) = fallible!(mon, client.get_group_settings(key).await) else {
        mon.error(format!(
            "Couldn't find group settings for `{key}`; skipping..."
        ));
        return Ok(());
    };

    let mut truncated_description = group.description_sv.clone();
    truncated_description.truncate(4096); // max supported by Google Groups

    let mut alt_description = group.description_en.clone();
    alt_description.truncate(4096);

    let target = google::GroupSettings {
        name: group.name_sv.clone(),
        description: truncated_description,
        who_can_view_group: google::GroupVisibility::AllMembersCanView,
        who_can_view_membership: google::GroupVisibility::AllMembersCanView,
        who_can_discover_group: google::GroupDiscoverability::AllInDomainCanDiscover,
        who_can_join: google::GroupJoinPermission::InvitedCanJoin,
        who_can_leave_group: google::GroupLeavePermission::NoneCanLeave,
        who_can_contact_owner: google::GroupContactOwnerPermission::AllMembersCanContact,
        who_can_post_message: google::GroupPostPermission::AnyoneCanPost,
        who_can_moderate_members: google::GroupModerationPermission::None,
        who_can_moderate_content: google::GroupModerationPermission::OwnersAndManagers,
        who_can_assist_content: google::GroupModerationPermission::AllMembers,
        allow_web_posting: true.into(),
        allow_external_members: false.into(),
        is_archived: true.into(),
        members_can_post_as_the_group: false.into(),
        enable_collaborative_inbox: true.into(),
        message_moderation_level: google::GroupMessageModerationLevel::ModerateNone,
        spam_moderation_level: google::GroupSpamModerationLevel::Moderate,
        default_sender: google::GroupDefaultSender::DefaultSelf,
    };

    let Some(patch) =
        google::GroupSettingsPatch::new(&current, &target, &group.name_en, &alt_description)
    else {
        // nothing to update
        return Ok(());
    };

    mon.info(format!("Patching `{key}` group settings: {patch:?}"));

    if mode.should_update() {
        fallible!(mon, client.patch_group_settings(key, &patch).await);
    }

    Ok(())
}

async fn sync_group_members(
    key: &str,
    subgroup_emails: &[&str],
    direct_members: &HashSet<UserWithEmail>,
    client: &DirectoryApiClient,
    mode: Mode,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    let direct_member_emails: Vec<_> = direct_members.iter().map(|m| m.email.as_ref()).collect();

    let mut current = fallible!(mon, client.list_group_members(key).await);

    for entry in &mut current {
        entry.email = entry.email.to_lowercase();

        let present = match entry.r#type {
            google::GroupMemberType::Group => subgroup_emails.contains(&entry.email.as_str()),
            google::GroupMemberType::User => direct_member_emails.contains(&entry.email.as_str()),
        };

        if !present {
            mon.info(format!(
                "Removing member `{}` from group `{}`",
                entry.email, key
            ));

            if mode.should_delete() {
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

            if mode.should_insert() {
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

                if mode.should_update() {
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

            if mode.should_insert() {
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
            email: user.primary_email.to_lowercase(),
        }));
    }

    if !allow_external {
        mon.warn(format!(
            "Cannot use a personal email for `{username}` because group does not support external \
             members"
        ));

        return Ok(None);
    }

    let personal: Option<String> = sqlx::query_scalar(
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
            email: email.to_lowercase(),
        }))
    } else {
        Ok(None)
    }
}

#[derive(Hash, PartialEq, Eq)]
struct UserWithEmail {
    username: String,
    email: String,
}

impl UserWithEmail {
    fn new_extra(email: String) -> Option<Self> {
        if email.contains('@') {
            Some(UserWithEmail {
                username: format!("extra#{email}"),
                email: email.to_lowercase(),
            })
        } else {
            None
        }
    }
}
