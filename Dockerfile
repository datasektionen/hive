ARG RUST_VERSION=1.82
ARG DEBIAN_LTS=bookworm

########## COMPILE PHASE ##########
FROM rust:${RUST_VERSION}-slim-${DEBIAN_LTS} AS build

WORKDIR /hive

# this looks strange but makes subsequent builds much faster
# because it leverages:
#       - a cache mount to /usr/local/cargo/registry/ to avoid
#         re-downloading all dependencies every time;
#       - a cache mount to /hive/target to avoid re-compiling
#         all dependencies every time; and
#       - a bind mount to the sources to avoid copying them
#         into the container every time
# after build we need to copy the binary to the container
# filesystem before /hive/target is unmounted
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=./target \
    --mount=type=bind,source=./Cargo.toml,target=./Cargo.toml \
    --mount=type=bind,source=./Cargo.lock,target=./Cargo.lock \
    --mount=type=bind,source=./src,target=./src \
    --mount=type=bind,source=./locales,target=./locales \
    --mount=type=bind,source=./migrations,target=./migrations \
    --mount=type=bind,source=./templates,target=./templates \
    --mount=type=bind,source=./rinja.toml,target=./rinja.toml \
    \
    cargo build --locked --release \
    && cp ./target/release/hive .

########## RUN PHASE ##########
FROM debian:${DEBIAN_LTS}-slim AS final

ARG UID=10080
ARG USER=hive
ARG LOG_FILE=/var/log/hive.log

RUN adduser \
    --disabled-password \
    --no-create-home \
    --gecos "" \
    --home "/non-existent" \
    --shell "/sbin/nologin" \
    --uid "${UID}" \
    ${USER}

RUN touch ${LOG_FILE}
RUN chown ${USER} ${LOG_FILE}
ENV HIVE_LOG_FLE=${LOG_FILE}

USER ${USER}

WORKDIR /hive
COPY --from=build /hive/hive .
COPY ./static /hive/static

EXPOSE ${HIVE_PORT:-6869}

HEALTHCHECK --interval=1m --timeout=20s --retries=3 \
            --start-period=5s --start-interval=1s \
    CMD curl -f http://localhost:${HIVE_PORT} || exit 1

ENTRYPOINT [ "./hive" ]
