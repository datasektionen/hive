use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;

use crate::{
    dto::tags::AssignTagDto,
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{SimpleGroup, Tag, TagAssignment},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::groups::{self, AuthorityInGroup},
    web::{Either, RenderedTemplate},
};

pub fn routes() -> RouteTree {
    rocket::routes![list_tag_assignments, assign_tag].into()
}

#[derive(Template)]
#[template(path = "groups/tags/list.html.j2")]
struct ListTagAssignmentsView {
    ctx: PageContext,
    tag_assignments: Vec<TagAssignment>,
    can_manage_any: bool,
}

#[derive(Template)]
#[template(path = "groups/tags/assign.html.j2", block = "inner_assign_tag_form")]
struct PartialAssignTagView<'f, 'v> {
    ctx: PageContext,
    group: SimpleGroup,
    assignable_tags: Vec<Tag>,
    assign_tag_form: &'f form::Context<'v>,
    assign_tag_success: Option<TagAssignment>,
}

#[rocket::get("/group/<domain>/<id>/tags")]
pub async fn list_tag_assignments(
    id: &str,
    domain: &str,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    if partial.is_none() {
        // we only know how to render a table, not a full page;
        // redirect to group details

        let target = uri!(super::group_details(id = id, domain = domain));
        return Ok(Either::Right(Redirect::to(target)));
    }

    groups::details::require_authority(
        AuthorityInGroup::View,
        id,
        domain,
        db.inner(),
        perms,
        &user,
    )
    .await?;

    let tag_assignments = groups::tags::get_all_assignments(id, domain, db.inner(), perms).await?;

    // this could've been directly in the template, but askama doesn't seem
    // to support closures defined in the source (parsing error)
    let can_manage_any = tag_assignments
        .iter()
        .any(|a| matches!(a.can_manage, Some(true)));

    let template = ListTagAssignmentsView {
        ctx,
        tag_assignments,
        can_manage_any,
    };

    Ok(Either::Left(RawHtml(template.render()?)))
}

#[rocket::post("/group/<domain>/<id>/tags", data = "<form>")]
#[allow(clippy::too_many_arguments)]
pub async fn assign_tag<'v>(
    id: &str,
    domain: &str,
    form: Form<Contextual<'v, AssignTagDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    let group = groups::details::require_one(id, domain, db.inner()).await?;

    let assignable_tags = groups::tags::get_all_assignable(perms, db.inner()).await?;

    if let Some(dto) = &form.value {
        // validation passed

        let min = HivePermission::AssignPerms(SystemsScope::Id(dto.tag.system_id.to_owned()));
        perms.require(min).await?;

        let assignment = groups::tags::assign(id, domain, dto, db.inner(), &user).await?;

        if partial.is_some() {
            let template = PartialAssignTagView {
                ctx,
                assign_tag_form: &form::Context::default(),
                assign_tag_success: Some(assignment),
                group,
                assignable_tags,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?added_tag=$key

            let target = uri!(super::group_details(id = id, domain = domain));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Assign tag form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = PartialAssignTagView {
                ctx,
                assign_tag_form: &form.context,
                assign_tag_success: None,
                group,
                assignable_tags,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(super::group_details(id = id, domain = domain));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}
