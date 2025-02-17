# build stage
FROM rust AS builder
WORKDIR /usr/src/mission-backend-rs
RUN apt-get update
RUN apt-get -y install libpq-dev lld
# https://www.aloxaf.com/2018/09/reduce_rust_size/
RUN apt-get -y install binutils
RUN wget https://github.com/upx/upx/releases/download/v4.2.4/upx-4.2.4-amd64_linux.tar.xz
RUN tar -xf upx-4.2.4-amd64_linux.tar.xz

COPY ./backend ./backend
COPY ./common ./common
COPY ./client ./client
COPY ./Cargo.toml ./Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
ENV RUSTFLAGS="-C link-arg=-fuse-ld=lld"

RUN cargo build --release --bin mission-backend-rs
RUN strip target/release/mission-backend-rs
RUN ./upx-4.2.4-amd64_linux/upx --best target/release/mission-backend-rs

# production stage
FROM debian:stable-slim
RUN apt-get update
RUN apt-get -y install libpq5
COPY --from=builder /usr/src/mission-backend-rs/target/release/mission-backend-rs /usr/local/bin/mission-backend-rs
CMD ["mission-backend-rs"]
