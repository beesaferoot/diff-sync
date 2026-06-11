# syntax=docker/dockerfile:1

# Stage 1 — build the Rust binary
FROM rust:1.83-bookworm AS rust-builder
WORKDIR /app
# Copy manifests + sources. rusqlite uses the `bundled` feature, so no system
# SQLite is needed to build; the default `network` feature pulls tokio/axum/tower-http.
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin ws-server && \
    cp target/release/ws-server /usr/local/bin/ws-server

# Stage 2 — build the Next.js static export (output: "export" -> /web/out)
FROM node:22-bookworm-slim AS web-builder
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ .
RUN npm run build

# Stage 3 — minimal runtime
FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl sqlite3 && \
    rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=rust-builder /usr/local/bin/ws-server ./ws-server
COPY --from=web-builder  /web/out ./web/out
EXPOSE 8080 8081
VOLUME /app/data
# 0.0.0.0 binds so the published ports are reachable from outside the container.
CMD ["./ws-server", \
     "--tcp-address",   "0.0.0.0:8080", \
     "--ws-address",    "0.0.0.0:8081", \
     "--database-path", "/app/data/documents.db", \
     "--static-dir",    "web/out"]
