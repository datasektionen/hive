UPDATE "permission_assignments"
SET perm_id = 'manage-perms'
WHERE system_id = 'hive' AND perm_id = 'assign-perms';

DELETE FROM "permissions"
WHERE system_id = 'hive' AND perm_id = 'assign-perms';

UPDATE "permissions"
SET description = 'Manage permissions for a given system'
WHERE system_id = 'hive' AND perm_id = 'manage-perms';
