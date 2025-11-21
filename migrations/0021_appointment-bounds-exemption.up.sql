-- This is not strictly necessary since Hive won't mind if the tag doesn't
-- exist, but it's a nice QoL for it to always be there (even on new setups)
-- and e.g. have a consistent description

INSERT INTO "tags"
    (system_id, tag_id, supports_users, supports_groups, has_content, description)
VALUES
    (
        'hive',
        'appointment-bounds-exemption',
        FALSE,
        TRUE,
        FALSE,
        'Group allowing unbounded long-term appointment'
    );
