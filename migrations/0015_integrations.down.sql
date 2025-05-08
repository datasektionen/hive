DELETE FROM "permissions"
WHERE system_id = 'hive'
    AND perm_id = 'manage-integration'; -- also cascades assignments

DROP TABLE "integration_task_logs";

DROP TYPE "integration_task_log_entry_kind";

DROP TABLE "integration_task_runs";

DROP TABLE "integration_settings";
