# Multi-stage build for EpiKairos Rust Backend
FROM rust:1-slim AS builder

WORKDIR /usr/src/epikairos
COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/

RUN cargo build --release

# Final runtime image
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/epikairos/target/release/epikairos /app/epikairos
COPY templates/ /app/templates/
COPY static/ /app/static/

ENV PORT=5000
ENV HOST=0.0.0.0
ENV FEEDS_DB_PATH=/app/data/feeds.db
ENV SCHOOL_DB_PATH=/app/data/school.db
ENV SECRET_DIR=/app/data

RUN mkdir -p /app/data

EXPOSE 5000

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:5000/health || exit 1

CMD ["/app/epikairos"]
