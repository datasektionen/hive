DELETE FROM "tags"
WHERE system_id = 'hive'
    AND tag_id = 'sensitive';
-- ^ this cascades to tag_assignments
