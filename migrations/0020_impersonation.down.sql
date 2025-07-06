DELETE FROM "permissions"
WHERE system_id = 'hive'
    AND perm_id = 'impersonate-users';
-- ^ this cascades to permission_assignments


-- Postgres doesn't support removing enum values, so we just keep it,
-- which should be fine since the UP migration only adds IF NOT EXISTS
