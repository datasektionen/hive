DELETE FROM "tags"
WHERE system_id = 'hive'
    AND tag_id = 'appointment-bounds-exemption';
-- ^ this cascades to tag_assignments
