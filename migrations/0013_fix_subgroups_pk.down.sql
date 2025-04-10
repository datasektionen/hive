-- Recreate the old (wrong) 2-column PK

ALTER TABLE "subgroups"
    DROP CONSTRAINT subgroups_pkey,
    ADD PRIMARY KEY (parent_id, child_id);
