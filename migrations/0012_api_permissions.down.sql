DELETE FROM "permissions"
WHERE system_id = 'hive'
    AND perm_id IN ('api-check-permissions', 'api-list-tagged');
-- ^ this cascades to permission_assignments
