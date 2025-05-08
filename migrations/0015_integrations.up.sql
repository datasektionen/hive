-- There's no master `integrations` table because it wouldn't make sense to
-- list or remember integrations that the codebase doesn't recognize, since
-- the manifest is essential for any operations. Thus, only actual state is
-- stored in the database (though it's assumed that each integration has a
-- matching system with the same ID)

CREATE TABLE "integration_settings" (
    integration_id SLUG  NOT NULL,
    setting_id     SLUG  NOT NULL,
    setting_value  JSONB NOT NULL,

    PRIMARY KEY (integration_id, setting_id),
    FOREIGN KEY (integration_id) REFERENCES "systems" (id) ON DELETE CASCADE
);

CREATE TABLE "integration_task_runs" (
    run_id UUID  PRIMARY KEY DEFAULT gen_random_uuid(),

    integration_id SLUG  NOT NULL,
    task_id        SLUG  NOT NULL,

    start_stamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    end_stamp   TIMESTAMPTZ,
    succeeded   BOOL,

    FOREIGN KEY (integration_id) REFERENCES "systems" (id) ON DELETE CASCADE,

    -- ensures no run starts while another is still running (both would have end=NULL)
    CONSTRAINT integration_task_lock UNIQUE NULLS NOT DISTINCT (integration_id, task_id, end_stamp)
);

CREATE TYPE "integration_task_log_entry_kind" AS ENUM (
    'error',
    'warning',
    'info'
);

CREATE TABLE "integration_task_logs" (
    entry_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id   UUID NOT NULL,

    kind    INTEGRATION_TASK_LOG_ENTRY_KIND NOT NULL,
    stamp   TIMESTAMPTZ                     NOT NULL DEFAULT NOW(),
    message TEXT                            NOT NULL,

    FOREIGN KEY (run_id) REFERENCES "integration_task_runs" (run_id) ON DELETE CASCADE
);

INSERT INTO "permissions" (system_id, perm_id, has_scope, description) VALUES
    ('hive', 'manage-integration', TRUE, 'Manage a specific Hive integration');

INSERT INTO "permission_assignments" (system_id, perm_id, scope, group_id, group_domain) VALUES
    ('hive', 'manage-integration', '*', 'root', 'hive.internal');
