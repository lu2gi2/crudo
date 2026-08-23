# qdrant-client currently requires a fairly recent stable Rust.
# Keep this pinned to avoid surprise breaks from `rust:latest`.
FROM rust:1.91-bookworm AS builder

WORKDIR /repo
COPY . /repo/rust/

WORKDIR /repo/rust
# Sanity check toolchain version (helps debug CI/Docker Desktop issues).
RUN rustc --version && cargo --version
# Build the service with qdrant support enabled (works even if you don't use qdrant).
RUN cargo build -p crudo-rag-service --release --features qdrant-index

FROM debian:bookworm-slim

WORKDIR /app
COPY --from=builder /repo/rust/target/release/crudo-rag-service /app/crudo-rag-service

EXPOSE 8787
ENTRYPOINT ["/app/crudo-rag-service"]
