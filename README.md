# Hive

Datasektionen's management system for groups and permissions.

Possible configuration settings are listed via `--help`, with corresponding
environment variables and `hive.toml` settings.

**Additionally, it is imperative that the `TZ` environment variable is set
correctly!** The local timezone is used to calculate group membership and thus
permissions. A recommended value is `TZ=Europe/Stockholm`.
