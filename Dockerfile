FROM rust:1.88 AS builder

WORKDIR /usr/src/app

RUN rustup target add x86_64-unknown-linux-musl
RUN rustup target add wasm32-unknown-unknown

RUN mkdir indexer
RUN mkdir front

RUN cargo install --locked trunk

COPY indexer/Cargo.toml indexer/Cargo.lock /usr/src/app/indexer/
COPY indexer/src /usr/src/app/indexer/src
COPY indexer/pg /usr/src/app/indexer/pg

RUN cd indexer && cargo build --target x86_64-unknown-linux-gnu --release

COPY front/Cargo.toml front/Cargo.lock front/index.html front/rust-toolchain front/Trunk.toml /usr/src/app/front/
COPY front/assets /usr/src/app/front/assets
COPY front/src /usr/src/app/front/src

RUN cd front && trunk build --release


FROM bitnami/minideb:bookworm

RUN apt-get update && apt-get install -y build-essential openssl libssl-dev ca-certificates

RUN mkdir /app
WORKDIR /app

COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-gnu/release/pumpfun_indexer /usr/local/bin/pumpfun_indexer
COPY --from=builder /usr/src/app/assets assets
COPY --from=builder /usr/src/app/pg pg

ARG POSTGRES_CONN_STR
ENV POSTGRES_CONN_STR=$POSTGRES_CONN_STR

ARG REDIS_CONN_STR
ENV REDIS_CONN_STR=$REDIS_CONN_STR

CMD ["pumpfun_indexer"]

EXPOSE 33987