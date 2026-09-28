# Тег bookworm = последний stable Rust на Debian Bookworm (2026: rustc ≥ 1.88 для icu_*, time и edition 2024)
FROM rust:bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/tnurs-remotedesk-web-api /usr/local/bin/
ENV RUST_LOG=tnurs_remotedesk_web_api=info
EXPOSE 8080
CMD ["tnurs-remotedesk-web-api"]
