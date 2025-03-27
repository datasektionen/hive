DELETE FROM "permissions"
WHERE system_id = 'hive'
    AND perm_id IN ('manage-tags', 'assign-tags');
-- ^ this cascades to permission_assignments

UPDATE "permissions"
SET description = 'Manage a specific system, but not permissions'
WHERE system_id = 'hive'
    AND perm_id = 'manage-system';
