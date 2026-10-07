FROM rust:1.90-bookworm AS build

WORKDIR /app

COPY . /app

# libclang is needed by bindgen to build RocksDB
RUN apt-get update && apt-get install -y libssl-dev pkg-config libclang-dev
# Docker is a pos
RUN cargo build --profile maxperf

FROM debian:bookworm

RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*

# Run as an unprivileged user. /app is its working directory, where the
# config is mounted and the cache databases are created.
RUN groupadd --system --gid 10001 blutgang \
    && useradd --system --uid 10001 --gid blutgang --home-dir /app --shell /usr/sbin/nologin blutgang \
    && mkdir /app \
    && chown blutgang:blutgang /app

COPY --from=build /app/target/maxperf/blutgang /usr/local/bin/blutgang
# Keep `./blutgang` working for commands written against the old layout
RUN ln -s /usr/local/bin/blutgang /app/blutgang

WORKDIR /app
USER 10001:10001
CMD ["blutgang", "-c", "config.toml"]
