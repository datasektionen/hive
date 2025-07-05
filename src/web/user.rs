use std::collections::HashMap;

use rinja::Template;
use rocket::{form::Form, response::content::RawHtml, State};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, user::User},
    routing::RouteTree,
    web::RenderedTemplate,
};

pub fn routes() -> RouteTree {
    rocket::routes![show_settings, update_settings].into()
}

#[derive(Template)]
#[template(path = "user/settings.html.j2")]
struct SettingsView {
    ctx: PageContext,
    settings: HashMap<String, Option<String>>,
    // ^ generated dynamically
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
