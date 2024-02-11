FROM rust:1.76-bookworm AS build

WORKDIR /app

COPY . /app

RUN apt-get update && apt-get install -y libssl-dev pkg-config
# Docker is a pos
RUN --mount=type=cache,target=/app/target \
    RUSTFLAGS='-C target-cpu=native' cargo build --profile maxperf


FROM debian:bookworm

RUN mkdir /app
RUN apt-get update && apt-get install -y openssl ca-certificates

COPY --from=build /app/target/release/blutgang /app/blutgang

WORKDIR /app
CMD ["./blutgang", "-c", "config.toml"]