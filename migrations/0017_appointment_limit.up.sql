INSERT INTO "permissions" (system_id, perm_id, has_scope, description) VALUES
    ('hive', 'long-term-appointment', TRUE, 'Appoint group members until X months in the future');

INSERT INTO "permission_assignments" (system_id, perm_id, scope, group_id, group_domain) VALUES
    ('hive', 'long-term-appointment', '*', 'root', 'hive.internal');
