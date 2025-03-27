-- WARNING: this migration cannot be reversed!

ALTER TABLE "api_tokens"
ALTER COLUMN secret TYPE TEXT;

UPDATE "api_tokens"
SET secret = encode(
    sha256(
        -- below is just conversion of secret (formatted UUID with dashes)
        -- to BYTEA of raw bytes, so that it matches the Rust-side hashing
        -- of raw &[u8] via the Uuid struct. without this, the final hash
        -- would be completely different
        decode(
            replace(secret, '-', ''),
            'hex'
        )
    ),
    'hex'
);
