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

const PAGE_SIZE: u32 = 50;

pub fn routes() -> RouteTree {
    rocket::routes![get_audit_logs].into()
}

#[derive(Template)]
#[template(path = "logs/details.html.j2")]
struct ListLogsView<'r> {
    ctx: PageContext,
    filter: LogsFilterDto<'r>,
    actors: Vec<String>,
    ids: Vec<String>,
    logs: Vec<AuditLog>,
    next_page: u32,
}

#[derive(Template)]
#[template(path = "logs/log-cells.html.j2")]
struct ListLogsPartial<'r> {
    ctx: PageContext,
    logs: Vec<AuditLog>,
    filter: LogsFilterDto<'r>,
    next_page: u32,
}

#[rocket::get("/logs?<page>&<actor>&<id>&<action>&<target>&<from>&<until>&<order>")]
async fn get_audit_logs<'r>(
    db: &State<PgPool>,
    ctx: PageContext,
    perms: &PermsEvaluator,
    partial: Option<HxRequest<'_>>,
    page: Option<u32>,
    actor: Option<&'r str>,
    id: Option<&'r str>,
    action: Option<ActionKind>,
    target: Option<TargetKind>,
    from: Option<BrowserDateTimeDto>,
    until: Option<BrowserDateTimeDto>,
    order: bool,
) -> AppResult<RenderedTemplate> {
    perms.require(HivePermission::ViewLogs).await?;

    let page = page.unwrap_or(1);

    let filter = LogsFilterDto {
        actor,
        action: action.clone(),
        target: target.clone(),
        id,
        until: until.clone(),
        from: from.clone(),
        order,
    };

    let actors = audit_logs::list_actors(db.inner()).await?;
    let ids = audit_logs::list_target_ids(db.inner()).await?;

    let logs = audit_logs::get_logs_paged(
        db.inner(),
        &filter,
        page.saturating_sub(1) * PAGE_SIZE,
        PAGE_SIZE,
    )
    .await?;

    if partial.is_some() {
        let template = ListLogsPartial {
            ctx,
            logs,
            filter,
            next_page: page + 1,
        };

        Ok(RawHtml(template.render()?))
    } else {
        let template = ListLogsView {
            ctx,
            logs,
            filter,
            next_page: page + 1,
            actors,
            ids,
        };

        Ok(RawHtml(template.render()?))
    }
}
