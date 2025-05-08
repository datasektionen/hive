DELETE FROM "permissions"
WHERE system_id = 'hive'
    AND perm_id = 'long-term-appointment';
-- ^ this cascades to permission_assignments
