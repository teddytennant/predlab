# --- build ---
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY .cargo ./.cargo
COPY src ./src
RUN cargo build --release

# --- runtime ---
# HTTP-only client (no TLS), so plain glibc is enough — no curl/openssl needed.
FROM debian:bookworm-slim
RUN useradd -r -u 10001 app
COPY --from=builder /app/target/release/leaderboard /usr/local/bin/leaderboard
USER app
ENV BIND=0.0.0.0:8003
EXPOSE 8003
CMD ["leaderboard"]
