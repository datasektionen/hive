CREATE TABLE "subtags" (
    parent_id        SLUG NOT NULL,
    parent_system_id SLUG NOT NULL,
    child_id         SLUG NOT NULL,
    child_system_id  SLUG NOT NULL,

    PRIMARY KEY (parent_id, parent_system_id, child_id, child_system_id),
    FOREIGN KEY (parent_id, parent_system_id) REFERENCES "tags" (tag_id, system_id) ON DELETE CASCADE,
    FOREIGN KEY (child_id, child_system_id)   REFERENCES "tags" (tag_id, system_id) ON DELETE CASCADE,
    CHECK ((parent_id <> child_id) OR (parent_system_id <> child_system_id))
);


-- this intentionally includes self-ancestry! it's the easiest way to
-- ensure all tags are included in the view at least once
CREATE VIEW "tag_ancestry"
    (descendant_id, descendant_system_id, ancestor_id, ancestor_system_id) AS
    WITH RECURSIVE tag_hierarchy AS (
        -- base case: all tags are their own ancestors
        SELECT
            ts.tag_id    AS descendant_id,
            ts.system_id AS descendant_system_id,
            ts.tag_id    AS ancestor_id,
            ts.system_id AS ancestor_system_id
        FROM tags ts

        UNION -- removes duplicates (vs. UNION ALL)

        -- recursive step: get ancestors
        SELECT
            th.descendant_id,
            th.descendant_system_id,
            st.parent_id        AS ancestor_id,
            st.parent_system_id AS ancestor_system_id
        FROM tag_hierarchy th
        JOIN subtags st
            ON st.child_id = th.ancestor_id
                AND st.child_system_id = th.ancestor_system_id
    )
    SELECT * FROM tag_hierarchy;


CREATE VIEW "all_tag_assignments"
    (id, system_id, tag_id, content, username, group_id, group_domain) AS
    SELECT
        CASE
            WHEN th.descendant_id = th.ancestor_id
                AND th.descendant_system_id = th.ancestor_system_id
            THEN ta.id
            ELSE NULL -- if indirect assignment, id is NULL
        END AS id,

        th.ancestor_system_id AS system_id,
        th.ancestor_id        AS tag_id,

        CASE
            WHEN th.descendant_id = th.ancestor_id
                AND th.descendant_system_id = th.ancestor_system_id
            THEN ta.content
            ELSE NULL -- if indirect assignment, content is NULL
        END AS content,

        ta.username,
        ta.group_id,
        ta.group_domain
    FROM tag_assignments ta
    JOIN tag_ancestry th
        ON ta.tag_id = th.descendant_id
            AND ta.system_id = th.descendant_system_id;
