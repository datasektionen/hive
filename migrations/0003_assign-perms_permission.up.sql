INSERT INTO "permissions" (system_id, perm_id, has_scope, description) VALUES
    ('hive', 'assign-perms', TRUE, 'Assign and unassign a given system''s permissions');

UPDATE "permissions"
SET description = 'Manage what permissions a given system supports'
WHERE system_id = 'hive' AND perm_id = 'manage-perms';

INSERT INTO "permission_assignments" (system_id, perm_id, scope, group_id, group_domain) VALUES
    ('hive', 'assign-perms', '*', 'root', 'hive.internal');
