FROM rust:1.88 AS base

RUN cargo install cargo-chef

FROM base AS planner

WORKDIR /usr/src/app

RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml Cargo.lock ./
COPY src src
COPY assets assets
COPY pg pg

RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder

WORKDIR /usr/src/app

COPY Cargo.toml Cargo.lock ./
COPY src src
COPY assets assets
COPY pg pg

COPY --from=planner /usr/src/app/recipe.json recipe.json

RUN cargo chef cook --target x86_64-unknown-linux-gnu --release --recipe-path recipe.json

FROM bitnami/minideb:bookworm

RUN apt-get update && apt-get install -y build-essential openssl libssl-dev ca-certificates

RUN mkdir /app
WORKDIR /app

COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-gnu/release/pumpfun_indexer /usr/local/bin/pumpfun_indexer
COPY --from=builder /usr/src/app/assets assets
COPY --from=builder /usr/src/app/pg pg

ARG POSTGRES_CONN_STR
ENV POSTGRES_CONN_STR=$POSTGRES_CONN_STR

CMD ["pumpfun_indexer"]

EXPOSE 33987