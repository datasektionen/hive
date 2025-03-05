use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::content::RawHtml,
    State,
};
use sqlx::PgPool;

use crate::{
    dto::systems::CreateSystemDto,
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator},
    models::System,
    perms::HivePermission,
    routing::RouteTree,
    sanitizers::SearchTerm,
};

use super::RenderedTemplate;

pub fn routes() -> RouteTree {
    rocket::routes![list_systems, create_system].into()
}

#[derive(Template)]
#[template(path = "systems/list.html.j2")]
struct ListSystemsView<'q, 'f, 'v> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
    create_form: &'f form::Context<'v>,
    create_modal_open: bool,
}

// FIXME: separate Partial struct is only needed until the next Askama/Rinja
// release; after that use new attr `blocks` (feature-gated) to impl many
// methods for the same template struct
#[derive(Template)]
#[template(path = "systems/list.html.j2", block = "inner_systems_listing")]
struct PartialListSystemsView<'q> {
    ctx: PageContext,
    systems: Vec<System>,
    q: Option<&'q str>,
}

#[derive(Template)]
#[template(path = "systems/create.html.j2", block = "inner_create_form")]
struct PartialCreateSystemView<'f, 'v> {
    ctx: PageContext,
    create_form: &'f form::Context<'v>,
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
        let template = ListSystemsView {
            ctx,
            systems,
            q,
            create_form: &form::Context::default(),
            create_modal_open: false,
        };

        Ok(RawHtml(template.render()?))
    }
}

#[rocket::post("/systems", data = "<form>")]
async fn create_system<'v>(
    form: Form<Contextual<'v, CreateSystemDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<RenderedTemplate> {
    perms.require(HivePermission::ManageSystems).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        // TODO: add to audit logs in same transaction

        let id: String = sqlx::query_scalar(
            "INSERT INTO systems (id, description) VALUES ($1, $2) RETURNING id",
        )
        .bind(dto.id)
        .bind(dto.description)
        .fetch_one(db.inner())
        .await?;

        // TODO: redirect to get_system, maybe both htmx and normal if easy
        Ok(RawHtml(format!("<b>New ID:</b> {id}")))
    } else {
        // some errors are present; show the form again
        debug!("Create system form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialCreateSystemView {
                ctx,
                create_form: &form.context,
            };

            Ok(RawHtml(template.render()?))
        } else {
            let systems = sqlx::query_as("SELECT * FROM systems ORDER BY id")
                .fetch_all(db.inner())
                .await?;

            let template = ListSystemsView {
                ctx,
                systems,
                q: None,
                create_form: &form.context,
                create_modal_open: true,
            };

            Ok(RawHtml(template.render()?))
        }
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
