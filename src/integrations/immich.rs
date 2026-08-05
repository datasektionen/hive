use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
};

use chrono::{Datelike, Local};
use iter_tools::Itertools;
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    integrations::{
        Mode, fallible,
        immich::immich::{
            AddUsersToAlbumDto, AlbumResponseDto, AlbumUserAddDto, AlbumUserRole, ImmichAPIClient,
            UserResponseDto,
        },
    },
    resolver::IdentityResolver,
    services::groups,
};

mod immich;

// can't use const because it wouldn't support async fn pointers for tasks
pub static MANIFEST: LazyLock<super::Manifest> = LazyLock::new(|| {
    super::Manifest {
        id: "immich",
        description: "Share albums in Immich",
        settings: &[
            super::Setting {
                id: "api-key",
                secret: true,
                name: "API Key",
                description: "Immich API Key",
                r#type: super::SettingType::ShortText,
            },
            super::Setting {
                id: "immich-url",
                secret: false,
                name: "Immich API URL",
                description: "Url to the immich api",
                r#type: super::SettingType::ShortText,
            },
            super::Setting {
                id: "mode",
                secret: false,
                name: "Mode",
                description: "Level of structural mirroring to enforce",
                r#type: super::SettingType::Select(super::MODE_OPTIONS),
            },
        ],
        permissions: &[],
        tags: &[super::Tag {
            id: "share",
            description: "Share albums with the prefix based on the members when the pictures where taken",
            has_content: true,
            supports_groups: true,
            supports_users: false,
            self_service: false,
        }],
        tasks: &[
            super::Task {
                id: "share-n0llan",
                schedule: "0 0 0,12 * * *", // Every 12 hours
                func: |mon, settings, resolver, db| {
                    Box::pin(share_n0llan(mon, settings, resolver, db))
                },
            },
            super::Task {
                id: "share-group",
                schedule: "0 0 0 * * *", // Every day
                func: |mon, settings, resolver, db| {
                    Box::pin(share_group(mon, settings, resolver, db))
                },
            },
        ],
    }
});

async fn share_group(
    mon: &mut super::TaskRunMonitor,
    settings: super::SettingsValues,
    resolver: Arc<Option<IdentityResolver>>,
    db: PgPool,
) -> AppResult<()> {
    let mode: Mode = super::require_serde_setting!(mon, settings, "mode");

    let api_key = super::require_string_setting!(mon, settings, "api-key");

    let immich_url = super::require_string_setting!(mon, settings, "immich-url");

    let api_client = fallible!(
        mon,
        immich::ImmichAPIClient::new(immich_url.to_string(), api_key.to_string())
    );

    mon.warn(mode.informational_message());

    let immich_users: HashMap<String, UserResponseDto> =
        fallible!(mon, api_client.list_users().await)
            .into_iter()
            .map(|user| (user.email.clone(), user))
            .collect();

    let albums = fallible!(mon, api_client.list_albums().await);

    let album_prefixes: HashMap<String, Vec<(String, String)>> = sqlx::query_as(
        "SELECT gs.id, gs.domain, ta.content
            FROM all_tag_assignments ta
            JOIN groups gs
                ON gs.id = ta.group_id
                    AND gs.domain = ta.group_domain
            WHERE ta.system_id = 'immich'
                AND ta.tag_id = 'share'
            ORDER BY gs.domain, gs.id",
    )
    .fetch_all(&db)
    .await?
    .into_iter()
    .map(|(id, domain, album_prefix)| (album_prefix, (id, domain)))
    .into_group_map();

    // There is an edgecase where multiple prefixes point to the same album which could cause a
    // problem
    for (album_prefix, groups) in album_prefixes {
        let albums: Vec<_> = albums
            .iter()
            .filter(|album| album.album_name.starts_with(&album_prefix))
            .collect();

        for album in albums {
            mon.info(format!("Sharing album `{}`", album.album_name));

            let mut usernames: HashSet<String> = HashSet::new();

            // Get all users who were members of the group any time in the timespan of the album
            for (id, domain) in &groups {
                let usernames_temp = sqlx::query_scalar(
                    r#"
                    -- direct members
                    SELECT
                        dm.username,
                        ARRAY[(dm.group_id, dm.group_domain)::GROUP_REF] AS path
                    FROM direct_memberships dm
                    WHERE dm.group_id = $1
                        AND dm.group_domain = $2
                        AND (dm.from, dm.until) OVERLAPS ($3, $4)

                    UNION -- removes duplicates (vs. UNION ALL)

                    -- indirect members
                    SELECT
                        dm.username,
                        sg.path || ($1, $2)::GROUP_REF AS path
                    FROM all_subgroups_of($1, $2) sg
                    JOIN direct_memberships dm
                        ON dm.group_id = sg.child_id
                        AND dm.group_domain = sg.child_domain
                        AND (dm.from, dm.until) OVERLAPS ($3, $4)
                "#,
                )
                .bind(&id)
                .bind(&domain)
                .bind(album.start_date)
                .bind(album.end_date)
                .fetch_all(&db)
                .await?;

                usernames.extend(usernames_temp);
            }

            let emails = if let Some(resolver) = resolver.as_ref() {
                resolver
                    .resolve_emails(usernames.iter().map(|s| s.as_str()))
                    .await?
            } else {
                continue;
            };

            let users: Vec<_> = emails
                .into_iter()
                .filter_map(|(_, email)| immich_users.get(&email))
                .collect();

            share_album(album, users, &api_client, mode, mon).await?;
        }
    }

    mon.info(format!("Shared {} albums!", albums.len()));

    Ok(())
}

async fn share_n0llan(
    mon: &mut super::TaskRunMonitor,
    settings: super::SettingsValues,
    resolver: Arc<Option<IdentityResolver>>,
    _db: PgPool,
) -> AppResult<()> {
    let mode: Mode = super::require_serde_setting!(mon, settings, "mode");

    let api_key = super::require_string_setting!(mon, settings, "api-key");

    let immich_url = super::require_string_setting!(mon, settings, "immich-url");

    let api_client = fallible!(
        mon,
        immich::ImmichAPIClient::new(immich_url.to_string(), api_key.to_string())
    );

    mon.warn(mode.informational_message());

    let immich_users: HashMap<String, UserResponseDto> =
        fallible!(mon, api_client.list_users().await)
            .into_iter()
            .map(|user| (user.email.clone(), user))
            .collect();

    let albums = fallible!(mon, api_client.list_albums().await);

    let reception_albums: Vec<_> = albums
        .iter()
        .filter(|album| album.album_name.starts_with("Mottagningen "))
        .collect();

    for album in reception_albums.iter() {
        mon.info(format!("Sharing album `{}`", album.album_name));

        // Extract the year from the album name
        let mut year = album.album_name.split_whitespace();
        year.next()
            .expect("already verified one word ending with space");

        let Some(year) = year.next() else {
            continue;
        };

        // Get the users from sso with the corresponding year tag, for the current year also look up
        // n0llan
        let users = if let Some(resolver) = resolver.as_ref() {
            let mut nollan = if Local::now().year().to_string() == year {
                resolver
                    .list_users_year("n0llan")
                    .await?
                    .into_iter()
                    .filter_map(|user| immich_users.get(&user.email))
                    .collect()
            } else {
                Vec::new()
            };

            let mut regular: Vec<_> = resolver
                .list_users_year(&format!("D-{}", year[2..].to_string()))
                .await?
                .into_iter()
                .filter_map(|user| immich_users.get(&user.email))
                .collect();

            regular.append(&mut nollan);

            regular
        } else {
            mon.error(format!(
                "Reception album name `{}` did not follow name standard",
                album.album_name
            ));
            continue;
        };

        share_album(album, users, &api_client, mode, mon).await?;
    }

    mon.info(format!("Shared {} albums!", reception_albums.len()));

    Ok(())
}

async fn share_album(
    album: &AlbumResponseDto,
    users: Vec<&UserResponseDto>,
    api_client: &ImmichAPIClient,
    mode: Mode,
    mon: &mut super::TaskRunMonitor,
) -> AppResult<()> {
    let mut album_users = album.album_users.clone();

    album_users.sort_unstable_by_key(|value| value.user.id.clone());

    for album_user in album_users.iter() {
        // Only remove viewers other roles are handled manualy
        if album_user.role != AlbumUserRole::Viewer {
            continue;
        }

        if users
            .binary_search_by_key(&album_user.user.id, |value| value.id.clone())
            .is_err()
        {
            mon.info(format!(
                "Removing `{}` from `{}`",
                album_user.user.name, album.album_name
            ));

            if mode.should_delete() {
                fallible!(
                    mon,
                    api_client.remove_user(&album.id, &album_user.user.id).await
                );
            }
        }
    }

    let users_to_add: Vec<_> = users
        .into_iter()
        .filter(|user| {
            album_users
                .binary_search_by_key(&user.id, |value| value.user.id.clone())
                .is_err()
        })
        .collect();

    for user in users_to_add.iter() {
        mon.info(format!(
            "Sharing `{}` with `{}`",
            album.album_name, user.name
        ));
    }

    let users_to_add: Vec<_> = users_to_add
        .into_iter()
        .map(|value| AlbumUserAddDto {
            role: AlbumUserRole::Viewer,
            user_id: value.id.clone(),
        })
        .collect();

    let body = AddUsersToAlbumDto {
        album_users: users_to_add,
    };

    if mode.should_insert() && body.album_users.len() > 0 {
        fallible!(mon, api_client.share_album(&album.id, body).await);
    }

    Ok(())
}
