FROM rust:1.99.0-alpine3.24 AS chef

RUN apk add --no-cache musl-dev \
  && cargo install cargo-chef --locked

WORKDIR /app

FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY .cargo ./.cargo

RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build

# Dependencies only; this layer is reused until the manifest or the lockfile changes.
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
  --mount=type=cache,id=knot-sync-target,target=/app/target,sharing=locked \
  cargo chef cook --release --locked --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY .cargo ./.cargo

# target/ lives in a cache mount, so the binary is copied out of it within the same step.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
  --mount=type=cache,id=knot-sync-target,target=/app/target,sharing=locked \
  cargo build --release --locked \
  && mkdir -p /out \
  && cp target/release/knot-sync /out/

FROM alpine:3.24.2

RUN apk add --no-cache ca-certificates \
  && addgroup -S knot-sync \
  && adduser -S -G knot-sync knot-sync

COPY --from=build /out/knot-sync /usr/local/bin/knot-sync

USER knot-sync

ENV KNOT_SYNC_LOG_FORMAT=json

CMD ["knot-sync"]
