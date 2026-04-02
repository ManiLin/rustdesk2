FROM rust:1.84-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/inventory-portal-api /usr/local/bin/
ENV RUST_LOG=inventory_portal_api=info
EXPOSE 8080
CMD ["inventory-portal-api"]
