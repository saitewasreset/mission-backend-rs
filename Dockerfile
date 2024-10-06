FROM rust as builder
WORKDIR /usr/src/mission-backend-rs
COPY ./migrations ./migrations
COPY ./src ./src
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
RUN cargo build --release --bin mission-backend-rs

FROM debian:stable-slim
RUN apt-get update && apt-get install -y libssl3 libpq5 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/mission-backend-rs/target/release/mission-backend-rs /usr/local/bin/mission-backend-rs
CMD ["mission-backend-rs"]
