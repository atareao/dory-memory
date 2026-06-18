# syntax=docker/dockerfile:1
FROM rust:1.85-slim-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY migrations/ migrations/

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release

FROM gcr.io/distroless/cc-debian12:latest

COPY --from=builder /app/target/release/dory-memory /usr/local/bin/dory-memory
COPY migrations/ /app/migrations/

EXPOSE 5005

ENV DORY_CONFIG=/etc/dory/dory.toml

ENTRYPOINT ["dory-memory"]