# World server and front door image (ADR 0045). Build from the repository root:
#   docker build -f deploy/Dockerfile -t arena-world:local .
# The browser client (dist/) is not in the image; the reverse proxy serves it.
FROM rust:1.99.1-slim-trixie AS build
WORKDIR /src
COPY . .
# `profiling` builds in the Pyroscope agent; it starts only when
# ARENA_PYROSCOPE_URL is set (ADR 0054).
RUN cargo build --release --locked -p arena-world --features profiling --bin arena-world --bin arena-door

FROM debian:trixie-slim
# CA certificates for HTTPS to OAuth providers (ADR 0055).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --no-create-home arena \
    && mkdir -p /var/lib/arena && chown arena:arena /var/lib/arena
COPY --from=build /src/target/release/arena-world /src/target/release/arena-door /usr/local/bin/
USER arena
EXPOSE 8080 8090
# SIGTERM starts the drain; give --drain-seconds and a longer stop timeout.
STOPSIGNAL SIGTERM
ENTRYPOINT ["arena-world"]
