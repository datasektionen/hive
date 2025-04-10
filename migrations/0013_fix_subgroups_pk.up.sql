-- Subgroups table was accidentally created using only child/parent IDs as PK,
-- without considering their domains, which is incorrect. This fixes that so
-- all 4 columns are considered and no erroneous uniqueness violations occur.

ALTER TABLE "subgroups"
    DROP CONSTRAINT subgroups_pkey,
    ADD PRIMARY KEY (parent_id, parent_domain, child_id, child_domain);
