INSERT INTO "permissions" (system_id, perm_id, has_scope, description) VALUES
    ('hive', 'impersonate-users', FALSE, 'Operate on Hive on behalf of any other user');

INSERT INTO "permission_assignments" (system_id, perm_id, scope, group_id, group_domain) VALUES
    ('hive', 'impersonate-users', NULL, 'root', 'hive.internal');

ALTER TYPE "action_kind" ADD VALUE IF NOT EXISTS 'impersonate';

ALTER TYPE "target_kind" ADD VALUE IF NOT EXISTS 'user';
