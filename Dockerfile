FROM docker.io/library/rust:alpine3.23 AS builder

RUN apk add --no-cache --update \
            build-base \
            pkgconfig \
            musl-dev \
            openssl \
            openssl-dev \
            openssl-libs-static

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY migrations/ migrations/

RUN cargo build --release && \
    strip target/release/dory-memory

FROM docker.io/library/alpine:3.23

RUN apk add --update --no-cache \
    ca-certificates \
    curl \
    openssl \
    && \
    adduser -S -u 1000 -D dory

COPY --from=builder /app/target/release/dory-memory /usr/local/bin/
COPY --from=builder /app/migrations/ /app/migrations/

USER dory
EXPOSE 5005

ENTRYPOINT ["dory-memory"]