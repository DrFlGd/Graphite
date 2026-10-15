# Build: docker build -f apps/relay/Dockerfile -t graphite-relay .
# Run:   docker run -p 7474:7474 -v relay-data:/data graphite-relay
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p graphite-relay

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/graphite-relay /usr/local/bin/graphite-relay
ENV RELAY_DATA=/data RELAY_ADDR=0.0.0.0:7474
VOLUME /data
EXPOSE 7474
CMD ["graphite-relay"]
