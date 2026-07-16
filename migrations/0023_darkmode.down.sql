DELETE FROM "tags"
WHERE system_id = 'hive'
    AND tag_id = 'darkmode';
-- ^ this cascades to tag_assignments
