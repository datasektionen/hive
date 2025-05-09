-- Convenience function to make it easier to calculate direct + indirect
-- group memberships, plus maybe allow Postgres to maybe optimize a bit

-- This is not a VIEW because it depends on the date and NOW() doesn't
-- (necessarily) reflect the application's timezone

-- (Note: this is similar to the existing `all_groups_of`, but this time
-- from the perspective of the group and not the user)

-- Since many different paths may exist, the same user may be listed
-- multiple times!

CREATE FUNCTION all_members_of(group_id SLUG, group_domain DOMAIN, at DATE)
RETURNS TABLE (username USERNAME, manager BOOL, path GROUP_REF[])
AS $$
    -- direct members
    SELECT
        dm.username,
        dm.manager,
        ARRAY[(dm.group_id, dm.group_domain)::GROUP_REF] AS path
    FROM direct_memberships dm
    WHERE dm.group_id = all_members_of.group_id
        AND dm.group_domain = all_members_of.group_domain

    UNION -- removes duplicates (vs. UNION ALL)

    -- indirect members
    SELECT
        dm.username,
        sg.manager,
        sg.path || (all_members_of.group_id, all_members_of.group_domain)::GROUP_REF AS path
    FROM all_subgroups_of(group_id, group_domain) sg
    JOIN direct_memberships dm
        ON dm.group_id = sg.child_id
        AND dm.group_domain = sg.child_domain
$$ LANGUAGE SQL;
