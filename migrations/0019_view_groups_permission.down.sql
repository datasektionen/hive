DELETE FROM "permissions"
WHERE system_id = 'hive'
    AND perm_id = 'view-groups';
-- ^ this cascades to permission_assignments
