-- Convenience function to make it easier to calculate direct + indirect
-- group memberships, plus maybe allow Postgres to maybe optimize a bit

-- This is not a VIEW because it depends on the date and NOW() doesn't
-- (necessarily) reflect the application's timezone

-- (Note: this is mostly used as an auxiliary function for `all_members_of`, by
-- recursively calculating all of a group's subgroups)

CREATE FUNCTION all_subgroups_of(parent_id SLUG, parent_domain DOMAIN, at DATE)
RETURNS TABLE (child_id SLUG, child_domain DOMAIN, manager BOOL, path GROUP_REF[])
AS $$
    WITH RECURSIVE subgroup_hierarchy(child_id, child_domain, manager, path) AS (
        SELECT
            sg.child_id,
            sg.child_domain,
            sg.manager,
            ARRAY[(sg.child_id, sg.child_domain)::GROUP_REF] AS path
        FROM subgroups sg
        WHERE sg.parent_id = all_subgroups_of.parent_id
            AND sg.parent_domain = all_subgroups_of.parent_domain

        UNION ALL -- removes duplicates (vs. UNION ALL)

        SELECT
            sg.child_id,
            sg.child_domain,
            sh.manager, -- just forward whether the first subgroup was a manager
            sh.path || (sg.child_id, sg.child_domain)::GROUP_REF AS path
        FROM subgroups sg
        JOIN subgroup_hierarchy sh
            ON sg.parent_id = sh.child_id
            AND sg.parent_domain = sh.child_domain
        WHERE NOT (sg.child_id, sg.child_domain)::GROUP_REF = ANY(sh.path) -- prevent cycles
    )
    SELECT * FROM subgroup_hierarchy
$$ LANGUAGE SQL;
