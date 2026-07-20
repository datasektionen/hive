use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use chrono::{Datelike, Local};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    integrations::{
        Mode, fallible,
        immich::immich::{AddUsersToAlbumDto, AlbumUserAddDto, AlbumUserRole, UserResponseDto},
    },
    resolver::IdentityResolver,
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
        tags: &[],
        tasks: &[super::Task {
            id: "share-albums",
            schedule: "0 0 * * * *", // every hour
            func: |mon, settings, resolver, db| Box::pin(share_albums(mon, settings, resolver, db)),
        }],
    }
});

async fn share_albums(
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
        mon.info(format!("Syncing album `{}`", album.album_name));

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
    }

    mon.info(format!("Synchronized {} albums!", reception_albums.len()));

    Ok(())
}
