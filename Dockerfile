FROM rust:1.60 as builder
WORKDIR /usr/src
RUN apt-get update && \
  apt-get dist-upgrade -y && \
  apt-get install -y \
    musl-tools \
    libssl-dev \
    build-essential \
    musl-dev \
    musl-tools \
    pkgconf \
    curl \
    zip \
    && \
  rustup target add x86_64-unknown-linux-musl
RUN USER=root cargo new tanker_price
WORKDIR /usr/src/tanker_price
COPY Cargo.toml Cargo.lock ./
# RUN cargo install --target x86_64-unknown-linux-musl --path .
COPY src ./src
# RUN cargo install --path .
RUN cargo install --target x86_64-unknown-linux-musl --path .

FROM scratch
COPY --from=builder /usr/local/cargo/bin/tanker_price .
USER 1000
ENTRYPOINT ["./tanker_price"]
