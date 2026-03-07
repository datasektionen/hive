use crate::{
    dto::{datetime::BrowserDateTimeDto, logs::LogsFilterDto},
    errors::AppResult,
    guards::{context::PageContext, headers::HxRequest, perms::PermsEvaluator},
    models::{ActionKind, AuditLog, TargetKind},
    perms::HivePermission,
    routing::RouteTree,
    services::audit_logs,
    web::RenderedTemplate,
};
use rinja::Template;
use rocket::Either;
use rocket::{response::content::RawHtml, State};
use sqlx::PgPool;

const PAGE_SIZE: i32 = 50;

pub fn routes() -> RouteTree {
    rocket::routes![get_audit_logs].into()
}

#[derive(Template)]
#[template(path = "logs/details.html.j2")]
struct LogsDetailsView<'r> {
    ctx: PageContext,
    actor_filter: Option<&'r str>,
    actors: Vec<String>,
    action_filter: Option<ActionKind>,
    target_filter: Option<TargetKind>,
    id_filter: Option<&'r str>,
    from_filter: Option<BrowserDateTimeDto>,
    until_filter: Option<BrowserDateTimeDto>,
    order: bool,
    ids: Vec<String>,
    logs: Vec<AuditLog>,
    next_page: i32
}

#[derive(Template)]
#[template(path = "logs/log-cells.html.j2")]
struct LogsPartial<'r> {
    ctx: PageContext,
    logs: Vec<AuditLog>,
    next_page: i32,
    actor_filter: Option<&'r str>,
    action_filter: Option<ActionKind>,
    target_filter: Option<TargetKind>,
    id_filter: Option<&'r str>,
    from_filter: Option<BrowserDateTimeDto>,
    until_filter: Option<BrowserDateTimeDto>,
    order: bool,
}

#[rocket::get("/logs?<page>&<actor>&<id>&<action>&<target>&<from>&<until>&<order>")]
async fn get_audit_logs<'r>(
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
    page: Option<i32>,
    actor: Option<&'r str>,
    id: Option<&'r str>,
    action: Option<ActionKind>,
    target: Option<TargetKind>,
    from: Option<BrowserDateTimeDto>,
    until: Option<BrowserDateTimeDto>,
    order: bool,
) -> AppResult<RenderedTemplate> {
    perms.require(HivePermission::ViewLogs).await?;

    let filters = LogsFilterDto {
        actor,
        action: action.clone(),
        target: target.clone(),
        id,
        until: until.clone(),
        from: from.clone(),
        order,
    };

    let actors = audit_logs::get_actors(db.inner()).await?;
    let ids = audit_logs::get_ids(db.inner()).await?;

    if let Some(page) = page {
        if partial.is_some() {
            let logs =
                audit_logs::get_logs_paged(db.inner(), &filters, page * PAGE_SIZE, PAGE_SIZE)
                    .await?;

            let template = LogsPartial {
                ctx,
                logs,
                next_page: page + 1,
                actor_filter: actor,
                id_filter: id,
                action_filter: action,
                target_filter: target,
                from_filter: from,
                until_filter: until,
                order,
            };

            Ok(RawHtml(template.render()?))
        } else {
            let logs =
                audit_logs::get_logs_paged(db.inner(), &filters, page * PAGE_SIZE, PAGE_SIZE)
                    .await?;

            let template = LogsDetailsView {
                ctx,
                logs,
                next_page: page + 1,
                actors,
                ids,
                actor_filter: actor,
                id_filter: id,
                action_filter: action,
                target_filter: target,
                from_filter: from,
                until_filter: until,
                order,
            };

            Ok(RawHtml(template.render()?))
        }
    } else {
        let logs = audit_logs::get_logs_paged(db.inner(), &filters, 0, PAGE_SIZE).await?;

        let template = LogsDetailsView {
            ctx,
            logs,
            next_page: 1,
            actors,
            ids,
            actor_filter: actor,
            id_filter: id,
            action_filter: action,
            target_filter: target,
            from_filter: from,
            until_filter: until,
            order,
        };

        Ok(RawHtml(template.render()?))
    }
}
