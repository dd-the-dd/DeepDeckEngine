FROM rust:1.97.1-bookworm AS builder
WORKDIR /source
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --locked --release --bin mtg-engine-server

FROM debian:bookworm-slim
RUN useradd --create-home --uid 10001 deepdeck
COPY --from=builder /source/target/release/mtg-engine-server /usr/local/bin/mtg-engine-server
USER deepdeck
ENV MTG_ENGINE_ADDR=0.0.0.0:8787
ENV MTG_UI_ADDR=
EXPOSE 8787
ENTRYPOINT ["mtg-engine-server"]
