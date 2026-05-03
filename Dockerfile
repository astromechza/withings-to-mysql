# ── Stage 1: planner (cargo-chef dependency fingerprint) ─────────────────────
FROM rust:1.88-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 2: build dependencies (cached layer) ───────────────────────────────
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
# Full source copy; only re-runs if src/** changes, not deps
COPY . .
RUN cargo build --release --bin withings-to-mysql

# ── Stage 3: minimal runtime ─────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/withings-to-mysql /usr/local/bin/withings-to-mysql
ENTRYPOINT ["/usr/local/bin/withings-to-mysql"]
