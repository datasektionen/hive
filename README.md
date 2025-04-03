# Hive

Datasektionen's management system for groups and permissions.

Possible configuration settings are listed via `--help`, with corresponding
environment variables and `hive.toml` settings.

**Additionally, it is imperative that the `TZ` environment variable is set
correctly!** The local timezone is used to calculate group membership and thus
permissions. A recommended value is `TZ=Europe/Stockholm`.

When compiled, the final binary includes all necessary information for runtime
execution **except** the `static/` directory, which must be provided at the
runtime working directory.

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

and then `docker compose down` when you're done. **If you would like live-reload
when you change something** (despite the unfortunate compilation speeds), you
can instead use:

```sh
docker compose up --watch
```

If you need to quickly change the configuration (e.g., the verbosity), this is a
good way to achieve that without rebuilding: create a `hive.toml` file and
Compose Watch will automatically sync it + restart the server.

## License

Copyright (c) 2025 Konglig Datasektionen

SPDX-License-Identifier: GPL-3.0-or-later
