-- This extends the previously-defined `all_members_of` to return, in addition
-- to the previously existing values, `from` and `until`, which are taken from
-- the user's direct membership to the path leaf group (not necessarily the
-- same as the queried group). This means that they are never NULL.

-- Remember that the same user can be listed multiple times, for each possible
-- path that makes them a member!

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

    UNION -- removes duplicates (vs. UNION ALL)

    -- indirect members
    SELECT
        dm.username,
        sg.manager,
        dm."from",
        dm."until",
        sg.path || (all_members_of.group_id, all_members_of.group_domain)::GROUP_REF AS path
    FROM all_subgroups_of(group_id, group_domain, at) sg
    JOIN direct_memberships dm
        ON dm.group_id = sg.child_id
        AND dm.group_domain = sg.child_domain
$$ LANGUAGE SQL;
