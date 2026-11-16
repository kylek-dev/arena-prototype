# Match server and front door image (ADR 0045). Build from the repository root:
#   docker build -f deploy/Dockerfile -t arena-server:local .
# The browser client (dist/) is not in the image; the reverse proxy serves it.
FROM rust:1.99.1-slim-trixie AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p arena-server --bin arena-server --bin arena-door

FROM debian:trixie-slim
RUN useradd --system --uid 10001 --no-create-home arena
COPY --from=build /src/target/release/arena-server /src/target/release/arena-door /usr/local/bin/
USER arena
EXPOSE 8080 8090
# SIGTERM starts the drain; give --drain-seconds and a longer stop timeout.
STOPSIGNAL SIGTERM
ENTRYPOINT ["arena-server"]
