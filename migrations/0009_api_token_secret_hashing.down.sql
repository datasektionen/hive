-- WARNING: this is a lossy migration!

-- It's not possible to reverse a hash operation, so our only choice is to
-- delete all data.

DELETE FROM "api_tokens";

ALTER TABLE "api_tokens"
ALTER COLUMN secret TYPE UUID USING secret::UUID;
