FROM rust:bookworm as builder
WORKDIR /usr/src/podmon
COPY . .
RUN cargo install --path .  --profile release
#RUN cargo install --path .

FROM debian:bookworm-slim
RUN apt-get update && apt install -y libssl-dev && apt install -y openssl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/podmon /usr/local/bin/podmon
CMD ["podmon"]
