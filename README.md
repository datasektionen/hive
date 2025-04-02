# Hive

Datasektionen's management system for groups and permissions.

Possible configuration settings are listed via `--help`, with corresponding
environment variables and `hive.toml` settings.

**Additionally, it is imperative that the `TZ` environment variable is set
correctly!** The local timezone is used to calculate group membership and thus
permissions. A recommended value is `TZ=Europe/Stockholm`.

## API

Hive is designed as a central single-source-of-truth that should be relied on by
other systems. It is intended for these other services to interact with Hive via
its HTTP REST API, which exposes several endpoints for different use-cases.

By default, API documentation will be included in the final binary and served at
route `/api/vX/docs`, for each supported version `X`.

If a smaller binary is desired and documentation is not necessary, you can build
Hive without it by disabling the `api-docs` Cargo feature with, e.g., the
`--no-default-features` flag for `cargo build`/`cargo run`.

## License

Copyright (c) 2025 Konglig Datasektionen

SPDX-License-Identifier: GPL-3.0-or-later
