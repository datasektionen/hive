use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use super::{Either, GracefulRedirect, RenderedTemplate};
use crate::{
    dto::tags::CreateTagDto,
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::Tag,
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::{systems, tags},
};

pub fn routes() -> RouteTree {
    rocket::routes![list_tags, create_tag, tag_details].into()
}

#[derive(Template)]
#[template(path = "tags/list.html.j2")]
struct ListTagsView {
    ctx: PageContext,
    tags: Vec<Tag>,
    can_manage: bool,
}

#[derive(Template)]
#[template(path = "tags/create.html.j2", block = "inner_create_tag_form")]
struct PartialCreateTagView<'f, 'v> {
    ctx: PageContext,
    tag_create_form: &'f form::Context<'v>,
}

#[derive(Template)]
#[template(path = "tags/details.html.j2")]
struct TagDetailsView {
    ctx: PageContext,
    tag: Tag,
    fully_authorized: bool,
}

#[rocket::get("/system/<system_id>/tags")]
async fn list_tags(
    system_id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    if partial.is_none() {
        // we only know how to render a table, not a full page;
        // redirect to system details

        let target = uri!(super::systems::system_details(system_id));
        return Ok(Either::Right(Redirect::to(target)));
    }

    perms
        .require_any_of(&[
            HivePermission::ManageSystems,
            HivePermission::ManageSystem(SystemsScope::Id(system_id.to_owned())),
            HivePermission::ManageTags(SystemsScope::Id(system_id.to_owned())),
        ])
        .await?;

    let tags = tags::list_for_system(system_id, db.inner()).await?;

    if tags.is_empty() {
        systems::ensure_exists(system_id, db.inner()).await?;
    }

    let template = ListTagsView {
        ctx,
        tags,
        can_manage: perms
            .satisfies_any_of(&[
                HivePermission::AssignTags(SystemsScope::Id(system_id.to_owned())),
                HivePermission::ManageTags(SystemsScope::Id(system_id.to_owned())),
            ])
            .await?,
    };

    Ok(Either::Left(RawHtml(template.render()?)))
}

#[rocket::post("/system/<system_id>/tags", data = "<form>")]
async fn create_tag<'v>(
    system_id: &str,
    form: Form<Contextual<'v, CreateTagDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, GracefulRedirect>> {
    let min = HivePermission::ManageTags(SystemsScope::Id(system_id.to_owned()));
    perms.require(min).await?;

    systems::ensure_exists(system_id, db.inner()).await?;

    // TODO: anti-CSRF

    if let Some(dto) = &form.value {
        // validation passed

        let tag = tags::create_new(system_id, dto, db.inner(), &user).await?;

        Ok(Either::Right(GracefulRedirect::to(
            uri!(tag_details(system_id = system_id, tag_id = tag.tag_id)),
            partial.is_some(),
        )))
    } else {
        // some errors are present; show the form again
        debug!("Create tag form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialCreateTagView {
                ctx,
                tag_create_form: &form.context,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(super::systems::system_details(system_id));
            Ok(Either::Right(GracefulRedirect::to(target, false)))
        }
    }
}

#[rocket::get("/system/<system_id>/tag/<tag_id>")]
async fn tag_details(
    system_id: &str,
    tag_id: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
) -> AppResult<RenderedTemplate> {
    let possibilities = [
        HivePermission::AssignTags(SystemsScope::Id(system_id.to_owned())),
        HivePermission::ManageTags(SystemsScope::Id(system_id.to_owned())),
    ];

    perms.require_any_of(&possibilities).await?;

    let tag = tags::require_one(system_id, tag_id, db.inner()).await?;

    let min = possibilities.into_iter().last().unwrap();
    let template = TagDetailsView {
        ctx,
        tag,
        fully_authorized: perms.satisfies(min).await?,
    };

    Ok(RawHtml(template.render()?))
}
