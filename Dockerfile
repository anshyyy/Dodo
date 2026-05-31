FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY openapi.yaml ./
COPY assets ./assets
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/dodo-invoice-service /app/dodo-invoice-service
COPY openapi.yaml ./openapi.yaml
COPY migrations ./migrations
COPY docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh
ENV LISTEN_ADDR=0.0.0.0:8080
ENV OPENAPI_SPEC_PATH=/app/openapi.yaml
EXPOSE 8080
ENTRYPOINT ["/app/docker-entrypoint.sh"]
