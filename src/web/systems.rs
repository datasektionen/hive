use rinja::Template;
use rocket::{response::content::RawHtml, State};
use sqlx::PgPool;

use crate::{
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator},
    models::System,
    perms::HivePermission,
    routing::RouteTree,
    sanitizers::SearchTerm,
};

use super::RenderedTemplate;

pub fn routes() -> RouteTree {
    rocket::routes![list_systems].into()
}

#[derive(Template)]
#[template(path = "systems/list.html.j2")]
struct ListSystemsView<'q> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
}

// FIXME: separate Partial struct is only needed until the next Askama/Rinja
// release; after that use new attr `blocks` (feature-gated) to impl many
// methods for the same template struct
#[derive(Template)]
#[template(path = "systems/list.html.j2", block = "partial")]
struct PartialListSystemsView<'q> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
}

#[rocket::get("/systems?<q>")]
async fn list_systems(
    q: Option<&str>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: support partial listing; ManageSystem(something)
    // (use `let systems = if all { query all } else { query some }`)

    let mut query = sqlx::QueryBuilder::new("SELECT * FROM systems");

    if let Some(search) = q {
        // this will push the same bind twice even though both could be
        // references to the same $1 param... there doesn't seem to be a
        // way to avoid this, since push_bind adds $n to the query itself
        let term = SearchTerm::from(search).anywhere();
        query.push(" WHERE id ILIKE ");
        query.push_bind(term.clone());
        query.push(" OR description ILIKE ");
        query.push_bind(term);
    }

    let systems = query.build_query_as().fetch_all(db.inner()).await?;

    if partial.is_some() {
        let template = PartialListSystemsView { ctx, systems, q };

        Ok(RawHtml(template.render()?))
    } else {
        let template = ListSystemsView { ctx, systems, q };

        Ok(RawHtml(template.render()?))
    }
}

mod filters {
    use regex::RegexBuilder;
    use rinja::filters::Safe;

    pub fn highlight<T: ToString>(s: Safe<T>, term: &str) -> rinja::Result<Safe<String>> {
        let s = s.0.to_string();

        let result = if term.is_empty() {
            s
        } else {
            let re = RegexBuilder::new(&regex::escape(term))
                .case_insensitive(true)
                .build()
                .unwrap();

            re.replace_all(&s, "<mark>$0</mark>").to_string()
        };

        Ok(Safe(result))
    }
}
