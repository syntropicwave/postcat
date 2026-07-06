# Build a single static-ish binary, then run it on a tiny base image.
FROM rust:1-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:stable-slim
RUN useradd -r -u 10001 postcat && mkdir /data && chown postcat /data
COPY --from=build /src/target/release/postcat-sync /usr/local/bin/postcat-sync
USER postcat
ENV POSTCAT_SYNC_DB=/data/postcat-sync.db
ENV POSTCAT_SYNC_ADDR=0.0.0.0:8787
VOLUME ["/data"]
EXPOSE 8787
ENTRYPOINT ["postcat-sync"]
