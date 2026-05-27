# Build context is the REPO ROOT (see docker-compose.yml) so build.rs can embed
# the canonical examples/predlab.py that we serve at /predlab.py.

# --- build ---
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY leaderboard-rs/Cargo.toml leaderboard-rs/Cargo.lock ./leaderboard-rs/
COPY leaderboard-rs/.cargo ./leaderboard-rs/.cargo
COPY leaderboard-rs/build.rs ./leaderboard-rs/build.rs
COPY leaderboard-rs/src ./leaderboard-rs/src
# Embedded by build.rs as ../examples/predlab.py relative to the crate.
COPY examples/predlab.py ./examples/predlab.py
WORKDIR /app/leaderboard-rs
RUN cargo build --release

# --- runtime ---
# HTTP-only client (no TLS), so plain glibc is enough — no curl/openssl needed.
FROM debian:bookworm-slim
RUN useradd -r -u 10001 app
COPY --from=builder /app/leaderboard-rs/target/release/leaderboard /usr/local/bin/leaderboard
USER app
ENV BIND=0.0.0.0:8003
EXPOSE 8003
CMD ["leaderboard"]
