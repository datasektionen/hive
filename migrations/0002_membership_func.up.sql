-- Convenience function to make it easier to calculate direct + indirect
-- group memberships, plus maybe allow Postgres to maybe optimize a bit

-- This is not a VIEW because it depends on the date and NOW() doesn't 
-- (necessarily) reflect the application's timezone

CREATE TYPE "group_ref" AS (group_id SLUG, group_domain DOMAIN);
-- ^ only because returning an array of `ROW(SLUG, DOMAIN)` isn't supported

CREATE FUNCTION all_groups_of(username USERNAME, at DATE)
RETURNS TABLE (id SLUG, domain DOMAIN, path GROUP_REF[])
AS $$
    WITH RECURSIVE group_hierarchy(group_id, group_domain, path) AS (
        SELECT
            dm.group_id,
            dm.group_domain,
            ARRAY[(dm.group_id, dm.group_domain)::GROUP_REF]
        FROM direct_memberships dm
        WHERE dm.username = all_groups_of.username
        AND all_groups_of.at BETWEEN dm."from" AND dm."until" -- between is inclusive

        UNION -- removes duplicates (vs. UNION ALL)

        SELECT
            sg.parent_id AS group_id,
            sg.parent_domain AS group_domain,
            gh.path || (sg.parent_id, sg.parent_domain)::GROUP_REF AS path
        FROM subgroups sg
        JOIN group_hierarchy gh
            ON gh.group_id = sg.child_id
            AND gh.group_domain = sg.child_domain
        WHERE NOT (sg.parent_id, sg.parent_domain)::GROUP_REF = ANY(gh.path) -- prevent cycles
    )
    SELECT group_id AS id, group_domain AS domain, path
    FROM group_hierarchy
$$ LANGUAGE SQL;
