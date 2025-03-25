ALTER TABLE "permission_assignments"
ADD CONSTRAINT no_duplicate_assignments
    UNIQUE NULLS NOT DISTINCT (system_id, perm_id, scope, group_id, group_domain, api_token_id);
