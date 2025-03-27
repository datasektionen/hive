use log::*;
use rinja::Template;
use rocket::{
    form::{self, Contextual, Form},
    response::{content::RawHtml, Redirect},
    uri, State,
};
use sqlx::PgPool;
use uuid::Uuid;

use super::{Either, GracefulRedirect, RenderedTemplate};
use crate::{
    dto::tags::{AssignTagToGroupDto, AssignTagToUserDto, CreateTagDto},
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator, user::User},
    models::{AffiliatedTagAssignment, Tag},
    perms::{HivePermission, SystemsScope},
    routing::RouteTree,
    services::{systems, tags},
};

pub fn routes() -> RouteTree {
    rocket::routes![
        list_tags,
        create_tag,
        tag_details,
        delete_tag,
        list_tag_groups,
        list_tag_users,
        assign_tag_to_group,
        assign_tag_to_user,
        unassign_tag
    ]
    .into()
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
struct TagDetailsView<'f, 'v> {
    ctx: PageContext,
    tag: Tag,
    fully_authorized: bool,
    assign_to_group_form: &'f form::Context<'v>,
    assign_to_group_success: Option<AffiliatedTagAssignment>,
    assign_to_user_form: &'f form::Context<'v>,
    assign_to_user_success: Option<AffiliatedTagAssignment>,
}

#[derive(Template)]
#[template(path = "tags/groups/list.html.j2")]
struct PartialListTagGroupsView {
    ctx: PageContext,
    has_content: bool,
    can_manage_any: bool,
    tag_assignments: Vec<AffiliatedTagAssignment>,
}

#[derive(Template)]
#[template(path = "tags/users/list.html.j2")]
struct PartialListTagUsersView {
    ctx: PageContext,
    has_content: bool,
    can_manage_any: bool,
    tag_assignments: Vec<AffiliatedTagAssignment>,
}

#[derive(Template)]
#[template(
    path = "tags/groups/assign.html.j2",
    block = "inner_assign_to_group_form"
)]
struct AssignTagToGroupView<'f, 'v> {
    ctx: PageContext,
    tag: Tag,
    assign_to_group_form: &'f form::Context<'v>,
    assign_to_group_success: Option<AffiliatedTagAssignment>,
}

#[derive(Template)]
#[template(
    path = "tags/users/assign.html.j2",
    block = "inner_assign_to_user_form"
)]
struct AssignTagToUserView<'f, 'v> {
    ctx: PageContext,
    tag: Tag,
    assign_to_user_form: &'f form::Context<'v>,
    assign_to_user_success: Option<AffiliatedTagAssignment>,
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

    let empty_form = form::Context::default();

    let min = possibilities.into_iter().last().unwrap();
    let template = TagDetailsView {
        ctx,
        tag,
        fully_authorized: perms.satisfies(min).await?,
        assign_to_group_form: &empty_form,
        assign_to_group_success: None,
        assign_to_user_form: &empty_form,
        assign_to_user_success: None,
    };

    Ok(RawHtml(template.render()?))
}

#[rocket::delete("/system/<system_id>/tag/<tag_id>")]
pub async fn delete_tag(
    system_id: &str,
    tag_id: &str,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<GracefulRedirect> {
    let min = HivePermission::ManageTags(SystemsScope::Id(system_id.to_owned()));
    perms.require(min).await?;

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    tags::delete(system_id, tag_id, db.inner(), &user).await?;

    // TODO: show visual confirmation of successful delete in tags list
    Ok(GracefulRedirect::to(
        uri!(super::systems::system_details(system_id)),
        partial.is_some(),
    ))
}

macro_rules! list_tag_assignments {
    ($path:expr, $fname:ident, $lister:path, $template:ident) => {
        #[rocket::get($path)]
        async fn $fname(
            system_id: &str,
            tag_id: &str,
            db: &State<PgPool>,
            ctx: PageContext,
            perms: &PermsEvaluator,
            partial: Option<HxRequest<'_>>,
        ) -> AppResult<Either<RenderedTemplate, Redirect>> {
            if partial.is_none() {
                // we only know how to render a table, not a full page;
                // redirect to tag details

                #[allow(clippy::redundant_locals)] // unclear why necessary
                let target = uri!(tag_details(system_id = system_id, tag_id = tag_id));
                return Ok(Either::Right(Redirect::to(target)));
            }

            perms
                .require_any_of(&[
                    HivePermission::AssignTags(SystemsScope::Id(system_id.to_owned())),
                    HivePermission::ManageTags(SystemsScope::Id(system_id.to_owned())),
                ])
                .await?;

            let has_content = tags::has_content(system_id, tag_id, db.inner()).await?;

            let tag_assignments =
                $lister(system_id, tag_id, Some(&ctx.lang), db.inner(), perms).await?;

            // this could've been directly in the template, but askama doesn't seem
            // to support closures defined in the source (parsing error)
            let can_manage_any = tag_assignments
                .iter()
                .any(|a| matches!(a.can_manage, Some(true)));

            let template = $template {
                ctx,
                has_content,
                can_manage_any,
                tag_assignments,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        }
    };
}

list_tag_assignments!(
    "/system/<system_id>/tag/<tag_id>/groups",
    list_tag_groups,
    tags::list_group_assignments,
    PartialListTagGroupsView
);

list_tag_assignments!(
    "/system/<system_id>/tag/<tag_id>/users",
    list_tag_users,
    tags::list_user_assignments,
    PartialListTagUsersView
);

#[rocket::post("/system/<system_id>/tag/<tag_id>/groups", data = "<form>")]
#[allow(clippy::too_many_arguments)]
async fn assign_tag_to_group<'v>(
    system_id: &str,
    tag_id: &str,
    form: Form<Contextual<'v, AssignTagToGroupDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    let min = HivePermission::AssignTags(SystemsScope::Id(system_id.to_string()));
    perms.require(min).await?;

    // TODO: anti-CSRF

    let tag = tags::require_one(system_id, tag_id, db.inner()).await?;

    if let Some(dto) = &form.value {
        // validation passed

        let assignment =
            tags::assign_to_group(system_id, tag_id, dto, Some(&ctx.lang), db.inner(), &user)
                .await?;

        if partial.is_some() {
            let template = AssignTagToGroupView {
                ctx,
                tag,
                assign_to_group_form: &form::Context::default(),
                assign_to_group_success: Some(assignment),
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?assigned_to_group=id@domain

            let target = uri!(tag_details(system_id = system_id, tag_id = tag_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Assign tag to group form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = AssignTagToGroupView {
                ctx,
                tag,
                assign_to_group_form: &form.context,
                assign_to_group_success: None,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(tag_details(system_id = system_id, tag_id = tag_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}

#[rocket::post("/system/<system_id>/tag/<tag_id>/users", data = "<form>")]
#[allow(clippy::too_many_arguments)]
async fn assign_tag_to_user<'v>(
    system_id: &str,
    tag_id: &str,
    form: Form<Contextual<'v, AssignTagToUserDto<'v>>>,
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<RenderedTemplate, Redirect>> {
    let min = HivePermission::AssignTags(SystemsScope::Id(system_id.to_string()));
    perms.require(min).await?;

    // TODO: anti-CSRF

    let tag = tags::require_one(system_id, tag_id, db.inner()).await?;

    if let Some(dto) = &form.value {
        // validation passed

        let assignment =
            tags::assign_to_user(system_id, tag_id, dto, Some(&ctx.lang), db.inner(), &user)
                .await?;

        if partial.is_some() {
            let template = AssignTagToUserView {
                ctx,
                tag,
                assign_to_user_form: &form::Context::default(),
                assign_to_user_success: Some(assignment),
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: maybe allow passing ?assigned_to_user=username

            let target = uri!(tag_details(system_id = system_id, tag_id = tag_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    } else {
        // some errors are present; show the form again
        debug!("Assign tag to user form errors: {:?}", &form.context);

        if partial.is_some() {
            let template = AssignTagToUserView {
                ctx,
                tag,
                assign_to_user_form: &form.context,
                assign_to_user_success: None,
            };

            Ok(Either::Left(RawHtml(template.render()?)))
        } else {
            // FIXME: this just resets the form without actually showing
            // any validation error indicators... but there isn't a great
            // alternative, and it might be fine for such a tiny form

            let target = uri!(tag_details(system_id = system_id, tag_id = tag_id));
            Ok(Either::Right(Redirect::to(target)))
        }
    }
}

#[rocket::delete("/tag-assignment/<id>")]
async fn unassign_tag(
    id: Uuid,
    db: &State<PgPool>,
    perms: &PermsEvaluator,
    user: User,
    partial: Option<HxRequest<'_>>,
) -> AppResult<Either<(), Redirect>> {
    // perms can only be checked later, not enough info now

    // TODO: anti-CSRF(?), DELETE isn't a normal form method

    let old = tags::unassign(id, db.inner(), perms, &user).await?;

    if partial.is_some() {
        Ok(Either::Left(()))
    } else {
        let target = uri!(tag_details(system_id = old.system_id, tag_id = old.tag_id));
        Ok(Either::Right(Redirect::to(target)))
    }
}
