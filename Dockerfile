FROM rust:1.86-bookworm AS backend-build
WORKDIR /build
COPY server server
RUN cargo build --locked --release --manifest-path server/Cargo.toml

FROM node:22-bookworm-slim AS web-build
WORKDIR /build
COPY client/package*.json ./
RUN npm ci
COPY client/ ./
RUN npm run build

FROM debian:bookworm-slim AS backend
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home mario
COPY --from=backend-build /build/server/target/release/mario-server /usr/local/bin/mario-server
USER mario
ENV MARIO_HOST=0.0.0.0
EXPOSE 4217 4218
ENTRYPOINT ["mario-server"]

FROM caddy:2-alpine AS web
COPY --from=web-build /build/dist /srv
COPY deploy/Caddyfile /etc/caddy/Caddyfile
EXPOSE 80 443
