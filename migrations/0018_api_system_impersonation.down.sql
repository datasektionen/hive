DELETE FROM "permissions"
WHERE system_id = 'hive'
    AND perm_id = 'api-impersonate-system';
-- ^ this cascades to permission_assignments
