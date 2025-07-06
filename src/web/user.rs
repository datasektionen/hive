use std::collections::HashMap;

use rinja::Template;
use rocket::{form::Form, response::content::RawHtml, State};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, perms::PermsEvaluator, user::User},
    models::{BasePermissionAssignment, SimpleGroup},
    perms::HivePermission,
    resolver::IdentityResolver,
    routing::RouteTree,
    services::{groups, permissions},
    web::RenderedTemplate,
};

pub fn routes() -> RouteTree {
    rocket::routes![show_profile, show_settings, update_settings].into()
}

#[derive(Template)]
#[template(path = "user/profile.html.j2")]
struct ProfileView<'a> {
    ctx: PageContext,
    own: bool,
    may_impersonate: bool,
    username: &'a str,
    display_name: String,
    known_groups: Vec<SimpleGroup>,
    permissions: Vec<BasePermissionAssignment>,
}

#[derive(Template)]
#[template(path = "user/settings.html.j2")]
struct SettingsView {
    ctx: PageContext,
    settings: HashMap<String, Option<String>>,
    // ^ generated dynamically
}

#[rocket::get("/user/<username>")]
async fn show_profile(
    username: &str,
    db: &State<PgPool>,
    resolver: &State<Option<IdentityResolver>>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
) -> AppResult<RenderedTemplate> {
    let own = user.username() == username;

    let may_impersonate = perms.satisfies(HivePermission::ImpersonateUsers).await?;

    let display_name = if let Some(resolver) = resolver.inner() {
        resolver.resolve_one(username).await?
    } else {
        None
    };

    let display_name = display_name.unwrap_or_else(|| {
        if own {
            user.display_name().to_owned()
        } else {
            "?".to_owned()
        }
    });

    let mut known_groups = vec![];

    for permissible in
        groups::list::list_all_permissible_sorted(&ctx.lang, db.inner(), perms, &user).await?
    {
        if groups::members::is_direct_member(
            username,
            &permissible.id,
            &permissible.domain,
            db.inner(),
        )
        .await?
        {
            known_groups.push(permissible);
        }
    }

    let permissions = permissions::list_all_assignments_for_user(username, db.inner()).await?;

    let template = ProfileView {
        ctx,
        own,
        may_impersonate,
        username,
        display_name,
        known_groups,
        permissions,
    };

    Ok(RawHtml(template.render()?))
}

// technically this URL prevents viewing the profile of a user named `settings`,
// but how likely is that to actually happen...
#[rocket::get("/user/settings")]
async fn show_settings(
    db: &State<PgPool>,
    ctx: PageContext,
    user: User,
) -> AppResult<RenderedTemplate> {
    let mut settings = HashMap::new();

    #[cfg(feature = "integrations")]
    {
        for manifest in &*crate::integrations::MANIFESTS {
            for tag in manifest.tags {
                if tag.self_service && tag.supports_users && tag.has_content {
                    use crate::services::integrations;

                    let value = integrations::get_self_service(
                        manifest.id,
                        tag.id,
                        user.username(),
                        db.inner(),
                    )
                    .await?;

                    // dots instead of underscores would look nicer, but we
                    // cannot use them because then we wouldn't be able to
                    // receive a proper HashMap<String, String> in mappings
                    // in the POST route, since rocket very helpfully(!)
                    // interprets dots for us as nesting
                    // (and we can't use dashes because slugs may contain them)
                    settings.insert(format!("integration_{}_{}", manifest.id, tag.id), value);
                }
            }
        }
    }

    let template = SettingsView { ctx, settings };

    Ok(RawHtml(template.render()?))
}

#[rocket::post("/user/settings", data = "<mappings>")]
async fn update_settings(
    mappings: Form<HashMap<String, String>>,
    db: &State<PgPool>,
    ctx: PageContext,
    user: User,
) -> AppResult<RenderedTemplate> {
    for (key, value) in mappings.into_inner() {
        #[cfg(feature = "integrations")]
        if let Some(scoped) = key.strip_prefix("integration_") {
            if let Some((integration_id, tag_id)) = scoped.split_once('_') {
                use crate::services::integrations;

                integrations::set_self_service(
                    integration_id,
                    tag_id,
                    user.username(),
                    &value,
                    db.inner(),
                )
                .await?;
            }
        }
    }

    show_settings(db, ctx, user).await
}
