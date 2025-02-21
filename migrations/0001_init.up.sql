CREATE DOMAIN "slug" AS TEXT CHECK (VALUE ~ '^[a-z0-9]+(-[a-z0-9]+)*$');

CREATE DOMAIN "domain" AS TEXT CHECK (VALUE ~ '^[-a-z0-9]+\.[a-z]+$');

CREATE DOMAIN "username" AS TEXT CHECK (VALUE ~ '^[a-z0-9]{2,}$');

CREATE TABLE "groups" (
    id             SLUG   NOT NULL,
    domain         DOMAIN NOT NULL,
    name_sv        TEXT   NOT NULL    CHECK (name_sv <> ''),
    name_en        TEXT   NOT NULL    CHECK (name_en <> ''),
    description_sv TEXT   NOT NULL    CHECK (description_sv <> ''),
    description_en TEXT   NOT NULL    CHECK (description_en <> ''),

    PRIMARY KEY (id, domain)
);

CREATE TABLE "direct_memberships" (
    id             UUID     PRIMARY KEY DEFAULT gen_random_uuid(),
    username       USERNAME NOT NULL,
    group_id       SLUG     NOT NULL,
    group_domain   DOMAIN   NOT NULL,
    "from"         DATE     NOT NULL,
    "until"        DATE     NOT NULL,
    manager        BOOL     NOT NULL DEFAULT FALSE,

    FOREIGN KEY (group_id, group_domain) REFERENCES "groups" (id, domain) ON DELETE CASCADE,
    CHECK ("from" <= "until")
);

COMMENT ON COLUMN "direct_memberships"."from"  IS 'inclusive';
COMMENT ON COLUMN "direct_memberships"."until" IS 'inclusive';

CREATE TABLE "subgroups" (
    parent_id     SLUG   NOT NULL,
    parent_domain DOMAIN NOT NULL,
    child_id      SLUG   NOT NULL,
    child_domain  DOMAIN NOT NULL,

    PRIMARY KEY (parent_id, child_id),
    FOREIGN KEY (parent_id, parent_domain) REFERENCES "groups" (id, domain) ON DELETE CASCADE,
    FOREIGN KEY (child_id, child_domain)   REFERENCES "groups" (id, domain) ON DELETE CASCADE,
    CHECK ((parent_id <> child_id) OR (parent_domain <> child_domain))
);

CREATE TABLE "systems" (
    id          SLUG PRIMARY KEY,
    description TEXT NOT NULL CHECK (description <> '')
);

CREATE TABLE "api_tokens" (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    secret      UUID UNIQUE      NOT NULL,
    system_id   SLUG             NOT NULL,
    description TEXT             NOT NULL CHECK (description <> ''),

    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,

    FOREIGN KEY (system_id) REFERENCES "systems" (id) ON DELETE CASCADE,
    CONSTRAINT no_token_ambiguity UNIQUE (system_id, description)
);

CREATE TABLE "tags" (
    system_id       SLUG NOT NULL,
    tag_id          SLUG NOT NULL,
    supports_users  BOOL NOT NULL,
    supports_groups BOOL NOT NULL,
    has_content     BOOL NOT NULL,
    description     TEXT NOT NULL CHECK (description <> ''),

    PRIMARY KEY (system_id, tag_id),
    FOREIGN KEY (system_id) REFERENCES "systems" (id) ON DELETE CASCADE,
    CHECK (supports_users OR supports_groups)
);

CREATE TABLE "tag_assignments" (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    system_id SLUG NOT NULL,
    tag_id    SLUG NOT NULL,
    content   TEXT          CHECK (content <> ''),

    username     USERNAME,
    group_id     SLUG,
    group_domain DOMAIN,

    FOREIGN KEY (system_id, tag_id)      REFERENCES "tags"   (system_id, tag_id) ON DELETE CASCADE,
    FOREIGN KEY (group_id, group_domain) REFERENCES "groups" (id, domain)        ON DELETE CASCADE,
    CONSTRAINT xor_user_group CHECK ((username IS NULL) <> (group_id IS NULL))
);

CREATE TABLE "permissions" (
    system_id   SLUG NOT NULL,
    perm_id     SLUG NOT NULL,
    has_scope   BOOL NOT NULL,
    description TEXT NOT NULL CHECK (description <> ''),

    PRIMARY KEY (system_id, perm_id),
    FOREIGN KEY (system_id) REFERENCES "systems" (id) ON DELETE CASCADE
);

CREATE TABLE "permission_assignments" (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    system_id SLUG NOT NULL,
    perm_id   SLUG NOT NULL,
    scope     TEXT          CHECK (scope <> ''),

    group_id     SLUG,
    group_domain DOMAIN,
    api_token_id UUID,

    FOREIGN KEY (system_id, perm_id)     REFERENCES "permissions" (system_id, perm_id) ON DELETE CASCADE,
    FOREIGN KEY (group_id, group_domain) REFERENCES "groups"      (id, domain)         ON DELETE CASCADE,
    FOREIGN KEY (api_token_id)           REFERENCES "api_tokens"  (id)                 ON DELETE CASCADE,
    CONSTRAINT xor_group_token CHECK ((group_id IS NULL) <> (api_token_id IS NULL))
);

COMMENT ON COLUMN "permission_assignments".scope IS 'Can be wildcard (*)';

CREATE TYPE "action_kind" AS ENUM ('create', 'update', 'delete');

CREATE TYPE "target_kind" AS ENUM (
    'group', 'membership',
    'system', 'api_token',
    'tag', 'tag_assignment',
    'permission', 'permission_assignment'
);

CREATE TABLE "audit_logs" (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    action_kind ACTION_KIND NOT NULL,
    target_kind TARGET_KIND NOT NULL,
    target_id   TEXT        NOT NULL CHECK (target_id <> ''),
    actor       USERNAME    NOT NULL CHECK (actor <> ''),
    stamp       TIMESTAMPTZ NOT NULL DEFAULT now(),
    details     JSONB       NOT NULL
);

----------------------------
----- Bootstrap values -----
----------------------------

INSERT INTO "systems" (id, description) VALUES ('hive', 'Organizational authorization management');

INSERT INTO "permissions" (system_id, perm_id, has_scope, description) VALUES
    ('hive', 'manage-groups', TRUE, 'Manage groups with #hive:tag or @domain'),
    ('hive', 'manage-members', TRUE, 'Manage members for groups with #hive:tag or @domain without being in the group'),
    ('hive', 'manage-systems', FALSE, 'Manage all systems'),
    ('hive', 'manage-system', TRUE, 'Manage a specific system, but not permissions'),
    ('hive', 'manage-perms', TRUE, 'Manage permissions for a given system'),
    ('hive', 'view-logs', FALSE, 'View centralized, global audit logs for all entities');

INSERT INTO "groups" (id, domain, name_sv, name_en, description_sv, description_en) VALUES
    ('root', 'hive.internal', 'Hive Administratörer', 'Hive Administrators',
     'Bootstrap-grupp med basbehörigheter på Hive', 'Bootstrap group with base permissions on Hive');

INSERT INTO "permission_assignments" (system_id, perm_id, scope, group_id, group_domain) VALUES
    ('hive', 'manage-groups', '*', 'root', 'hive.internal'),
    ('hive', 'manage-systems', NULL, 'root', 'hive.internal'),
    ('hive', 'manage-perms', '*', 'root', 'hive.internal'),
    ('hive', 'view-logs', NULL, 'root', 'hive.internal');

-- At startup, system should add first user into this 'root@hive.internal' group
-- iff the "direct_memberships" table is empty.
