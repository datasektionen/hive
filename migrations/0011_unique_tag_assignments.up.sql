ALTER TABLE "tag_assignments"
ADD CONSTRAINT no_duplicate_tag_assignments
    UNIQUE NULLS NOT DISTINCT (system_id, tag_id, content, group_id, group_domain, username);
