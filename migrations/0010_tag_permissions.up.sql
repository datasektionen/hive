UPDATE "permissions"
SET description = 'Manage a specific system, but not its permissions or tags'
WHERE system_id = 'hive'
    AND perm_id = 'manage-system';

INSERT INTO "permissions" (system_id, perm_id, has_scope, description) VALUES
    ('hive', 'manage-tags', TRUE, 'Manage what tags a given system supports'),
    ('hive', 'assign-tags', TRUE, 'Assign and unassign a given system''s tags');

INSERT INTO "permission_assignments" (system_id, perm_id, scope, group_id, group_domain) VALUES
    ('hive', 'manage-tags', '*', 'root', 'hive.internal'),
    ('hive', 'assign-tags', '*', 'root', 'hive.internal');
