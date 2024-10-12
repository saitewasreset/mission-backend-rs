FROM rust as builder
WORKDIR /usr/src/mission-backend-rs
COPY ./migrations ./migrations
COPY ./src ./src
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
RUN cargo build --release --bin mission-backend-rs

FROM alpine:latest
RUN apk add --no-cache openssl libpq
COPY --from=builder /usr/src/mission-backend-rs/target/release/mission-backend-rs /usr/local/bin/mission-backend-rs
CMD ["mission-backend-rs"]
