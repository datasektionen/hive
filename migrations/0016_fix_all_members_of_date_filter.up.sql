-- This fixes the previously defined function `all_members_of` to actually use
-- its `at` parameter, now returning only members at the specified date.

DROP FUNCTION all_members_of(group_id SLUG, group_domain DOMAIN, at DATE);

CREATE FUNCTION all_members_of(group_id SLUG, group_domain DOMAIN, at DATE)
RETURNS TABLE (username USERNAME, manager BOOL, "from" DATE, "until" DATE, path GROUP_REF[])
AS $$
    -- direct members
    SELECT
        dm.username,
        dm.manager,
        dm."from",
        dm."until",
        ARRAY[(dm.group_id, dm.group_domain)::GROUP_REF] AS path
    FROM direct_memberships dm
    WHERE dm.group_id = all_members_of.group_id
        AND dm.group_domain = all_members_of.group_domain
        AND all_members_of.at BETWEEN dm."from" AND dm."until" -- between is inclusive

    UNION -- removes duplicates (vs. UNION ALL)

    -- indirect members
    SELECT
        dm.username,
        sg.manager,
        dm."from",
        dm."until",
        sg.path || (all_members_of.group_id, all_members_of.group_domain)::GROUP_REF AS path
    FROM all_subgroups_of(group_id, group_domain) sg
    JOIN direct_memberships dm
        ON dm.group_id = sg.child_id
        AND dm.group_domain = sg.child_domain
        AND all_members_of.at BETWEEN dm."from" AND dm."until" -- between is inclusive
$$ LANGUAGE SQL;
