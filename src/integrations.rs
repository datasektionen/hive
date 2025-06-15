use std::{collections::HashMap, future::Future, pin::Pin, sync::LazyLock};

use chrono::Local;
use log::*;
use sqlx::{error::DatabaseError, PgPool};
use tokio_cron_scheduler::{Job, JobScheduler, JobSchedulerError};

use crate::{
    errors::AppResult,
    models::{IntegrationTaskLogEntry, IntegrationTaskLogEntryKind, IntegrationTaskRun},
};

#[cfg(feature = "integration-gworkspace")]
mod gworkspace;

// can't use const because it wouldn't support async fn pointers for tasks
pub static MANIFESTS: LazyLock<Vec<&Manifest>> = LazyLock::new(|| {
    vec![
        #[cfg(feature = "integration-gworkspace")]
        &*gworkspace::MANIFEST,
    ]
});

pub struct Manifest {
    pub id: &'static str,
    pub description: &'static str,
    pub settings: &'static [Setting],
    pub tags: &'static [Tag],
    pub tasks: &'static [Task],
}

pub struct Setting {
    pub id: &'static str,
    pub secret: bool,
    pub name: &'static str,
    pub description: &'static str,
    pub r#type: SettingType,
}

pub enum SettingType {
    Boolean,
    ShortText,
    LongText,
    Select(&'static [SelectSettingOption]),
}

pub struct SelectSettingOption {
    pub value: &'static str,
    pub display_name: &'static str,
}

pub struct Tag {
    pub id: &'static str,
    pub description: &'static str,
    pub has_content: bool,
    pub supports_groups: bool,
    pub supports_users: bool,
}

// Rust really is very clunky sometimes...
// We want to write ""async fn(&mut TaskMonitor, PgPool) -> AppResult<()>"",
// but of course that would be too easy (`async` keyword isn't allowed
// in function pointers), so we do this instead to make the function return
// a Future. However, since Future is a trait and async fn's return anonymous
// types, we actually need to box it.
type AppResultFuture<'a, T> = Pin<Box<dyn Future<Output = AppResult<T>> + Send + 'a>>;

pub struct Task {
    pub id: &'static str,
    pub schedule: &'static str,
    pub(self) func: fn(&mut TaskRunMonitor, SettingsValues, PgPool) -> AppResultFuture<'_, ()>,
}

type SettingsValues = HashMap<String, serde_json::Value>;

struct TaskRunMonitor {
    succeeded: bool,
    logs: Vec<IntegrationTaskLogEntry>,
}

impl TaskRunMonitor {
    fn new() -> Self {
        Self {
            succeeded: false,
            logs: Vec::with_capacity(128),
        }
    }

    fn succeeded(&mut self) {
        self.succeeded = true;
    }
}

macro_rules! impl_log_entry {
    ($name:ident, $kind:expr) => {
        impl TaskRunMonitor {
            fn $name<S: ToString>(&mut self, message: S) {
                let entry = IntegrationTaskLogEntry {
                    kind: $kind,
                    stamp: Local::now(),
                    message: message.to_string(),
                };

                self.logs.push(entry);
            }
        }
    };
}

impl_log_entry!(error, IntegrationTaskLogEntryKind::Error);
impl_log_entry!(warn, IntegrationTaskLogEntryKind::Warning);
impl_log_entry!(info, IntegrationTaskLogEntryKind::Info);

pub async fn schedule_tasks(db: PgPool) -> Result<(), JobSchedulerError> {
    let scheduler = JobScheduler::new().await?;

    for manifest in &*MANIFESTS {
        debug!("Setting up integration {} from manifest", manifest.id);

        setup_integration(manifest, &db).await;

        debug!("Registering jobs for integration {}", manifest.id);

        for task in manifest.tasks {
            let db = db.clone(); // cheap, just an Arc
            let job = Job::new_async_tz(task.schedule, Local, move |uuid, _| {
                let db = db.clone();

                Box::pin(async move {
                    debug!(
                        "Executing job {} for task {} (integration {})",
                        uuid, task.id, manifest.id
                    );

                    dispatch_task_run(manifest.id, task, &db)
                        .await
                        .expect("Task run failed");

                    debug!(
                        "Finished executing job {} for task {} (integration {})",
                        uuid, task.id, manifest.id
                    );
                })
            })?;

            scheduler.add(job).await?;
        }
    }

    debug!("Starting scheduler after successful registration");

    scheduler.start().await?;

    info!("All integration jobs scheduled!");

    Ok(())
}

async fn setup_integration(manifest: &Manifest, db: &PgPool) {
    sqlx::query(
        "INSERT INTO systems (id, description)
        VALUES ($1, $2)
        ON CONFLICT (id) DO UPDATE SET description = EXCLUDED.description",
    )
    .bind(manifest.id)
    .bind(manifest.description)
    .execute(db)
    .await
    .expect("Failed to create system for integration");

    // technically could do it in one query using UNNEST instead of looping,
    // but code would be way more confusing and #tags will likely be very low
    // anyway, so this is preferable
    for tag in manifest.tags {
        sqlx::query(
            "INSERT INTO tags
                (system_id, tag_id, description, supports_groups, supports_users, has_content)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (system_id, tag_id) DO UPDATE SET
                description = EXCLUDED.description,
                supports_groups = EXCLUDED.supports_groups,
                supports_users = EXCLUDED.supports_users,
                has_content = EXCLUDED.has_content",
        )
        .bind(manifest.id)
        .bind(tag.id)
        .bind(tag.description)
        .bind(tag.supports_groups)
        .bind(tag.supports_users)
        .bind(tag.has_content)
        .execute(db)
        .await
        .expect("Failed to create tag for integration");
    }
}

async fn dispatch_task_run(integration_id: &str, task: &Task, db: &PgPool) -> AppResult<()> {
    let run: IntegrationTaskRun = sqlx::query_as(
        "INSERT INTO integration_task_runs
            (integration_id, task_id)
        VALUES ($1, $2)
        RETURNING *",
    )
    .bind(integration_id)
    .bind(task.id)
    .fetch_one(db)
    .await
    .inspect_err(|e| {
        if e.as_database_error()
            .map(DatabaseError::is_unique_violation)
            .unwrap_or(false)
        {
            warn!("Run was aborted because another one is still ongoing");
        }
    })?;

    debug!("Assigned run ID {}", run.run_id);

    let settings: HashMap<String, serde_json::Value> = sqlx::query_as(
        "SELECT setting_id, setting_value
        FROM integration_settings
        WHERE integration_id = $1",
    )
    .bind(integration_id)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect();

    let mut mon = TaskRunMonitor::new();

    let result = (task.func)(&mut mon, settings, db.clone()).await;

    let mut txn = db.begin().await?;

    sqlx::query(
        "UPDATE integration_task_runs
        SET end_stamp = NOW(), succeeded = $1
        WHERE run_id = $2",
    )
    .bind(mon.succeeded)
    .bind(run.run_id)
    .execute(&mut *txn)
    .await?;

    let log_kinds: Vec<_> = mon.logs.iter().map(|entry| entry.kind).collect();
    let log_stamps: Vec<_> = mon.logs.iter().map(|entry| entry.stamp).collect();
    let log_msgs: Vec<_> = mon.logs.into_iter().map(|entry| entry.message).collect();

    sqlx::query(
        "INSERT INTO integration_task_logs (run_id, kind, stamp, message)
        SELECT * FROM UNNEST(
            $1::UUID[],
            $2::INTEGRATION_TASK_LOG_ENTRY_KIND[],
            $3::TIMESTAMPTZ[],
            $4::TEXT[]
        )",
    )
    .bind(vec![&run.run_id; log_msgs.len()])
    .bind(log_kinds)
    .bind(log_stamps)
    .bind(log_msgs)
    .execute(&mut *txn)
    .await?;

    txn.commit().await?;

    result
}

macro_rules! require_string_setting {
    ($mon:expr, $settings:expr, $key:literal) => {
        super::require_string_setting!($mon, $settings, $key, "")
    };
    ($mon:expr, $settings:expr, $key:literal, $contained:expr) => {{
        let value = $settings.get($key).and_then(|v| match v {
            serde_json::Value::String(s) if s.contains($contained) => Some(s),
            _ => None,
        });

        if let Some(value) = value {
            value
        } else {
            $mon.error(concat!("Setting value `", $key, "` is not set correctly"));

            return Ok(());
        }
    }};
}

// required to allow the `allow()` below
#[allow(clippy::useless_attribute)]
// required for usage in this module's children
#[allow(clippy::needless_pub_self)]
pub(self) use require_string_setting;
