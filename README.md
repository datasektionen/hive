# Hive

Datasektionen's management system for groups and permissions.

Possible configuration settings are listed via `--help`, with corresponding
`HIVE_` environment variables and `hive.toml` settings. In particular:

| **Setting**        | **Required** | **Format**                               |
| ------------------ | ------------ | ---------------------------------------- |
| Database URL       | **Yes**      | `postgresql://USER:PWD@HOST:PORT/DB`     |
| Secret Key         | **Yes**      | Generate with `openssl rand -hex 64`     |
| OIDC Issuer URL    | **Yes**      | e.g., `https://sso.datasektionen.se/op`  |
| OIDC Client ID     | **Yes**      | Ask authentication server provider       |
| OIDC Client Secret | **Yes**      | Ask authentication server provider       |
| Port               | No           | Default: `6869`                          |
| Listen Address     | No           | Default: `0.0.0.0` (listen everywhere)   |
| Verbosity          | No           | Default: `normal` (show warnings/errors) |
| Log File           | No           | Default: `/tmp/hive.log` (≠ in Docker)   |

**Additionally, it is imperative that the `TZ` environment variable is set
correctly!** The local timezone is used to calculate group membership and thus
permissions. A recommended value is `TZ=Europe/Stockholm`.

When compiled, the final binary includes all necessary information for runtime
execution **except** the `static/` directory, which must be provided at the
runtime working directory.

_**SECURITY NOTE:** Hive trusts the `Host` HTTP header of incoming requests to
be accurate, despite being client-controlled, so this can be used by an attacker
to man-in-the-middle OIDC logins unless you protect yourself against this:
outside development, **always make sure that Hive is served behind a reverse
proxy** (without wildcard host routing)!_

## API

Hive is designed as a central single-source-of-truth that should be relied on by
other systems. It is intended for these other services to interact with Hive via
its HTTP REST API, which exposes several endpoints for different use-cases.

By default, API documentation will be included in the final binary and served at
route `/api/vX/docs`, for each supported version `X`. **Visit `/api` to see a
listing of supported API versions and find links to their respective
documentation pages.**

If a smaller binary is desired and documentation is not necessary, you can build
Hive without it by disabling the `api-docs` Cargo feature with, e.g., the
`--no-default-features` flag for `cargo build`/`cargo run`.

## Development

Hive is written in Rust and so uses Cargo: you can run `cargo build` or
`cargo run` as normal.

However, testing usually requires a working database connection, so a Docker
Compose setup is provided to streamline having a working PostgreSQL instance.
**The easiest way to run Hive is thus** to execute (possibly with `sudo`):

```sh
docker compose up -d --build
```

and then `docker compose down` when you're done. _Note that the default Docker
Compose setup requires a `secrets.env` file, so you should
`cp secrets.env{.example,}` beforehand and fill in appropriate values._

**If you would like live-reload when you change something** (despite the
unfortunate compilation speeds), you can instead use:

```sh
docker compose up --watch
```

If you need to quickly change the configuration (e.g., the verbosity), this is a
good way to achieve that without rebuilding: create a `hive.toml` file and
Compose Watch will automatically sync it + restart the server.

## License

Copyright (c) 2025 Konglig Datasektionen

SPDX-License-Identifier: GPL-3.0-or-later
