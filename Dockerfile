ARG RUST_IMAGE=rust:1.88-bookworm@sha256:af306cfa71d987911a781c37b59d7d67d934f49684058f96cf72079c3626bfe0
ARG NODE_IMAGE=node:24.14.1-alpine@sha256:8510330d3eb72c804231a834b1a8ebb55cb3796c3e4431297a24d246b8add4d5
ARG DEBIAN_IMAGE=debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

FROM ${RUST_IMAGE} AS rust-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/myurl-server/Cargo.toml crates/myurl-server/Cargo.toml
RUN cargo fetch --locked
COPY crates crates
# Keep the test-only adapters available for isolated Docker integration tests.
# Production configuration still rejects TURNSTILE_MODE=test and TEST_STORE.
RUN cargo build --release --locked -p myurl-server --features test-support

FROM ${NODE_IMAGE} AS web-dependencies
WORKDIR /app
ENV COREPACK_HOME=/tmp/corepack
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY apps/web/package.json apps/web/package.json
RUN pnpm install --frozen-lockfile

FROM web-dependencies AS web-builder
COPY apps/web apps/web
RUN pnpm --filter @myurl/web build

FROM ${DEBIAN_IMAGE} AS runtime
ENV WEB_ROOT=/app/web
WORKDIR /app
RUN apt-get update && \
    apt-get install --yes --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --gid 10001 myurl && \
    useradd --uid 10001 --gid 10001 --no-create-home --shell /usr/sbin/nologin myurl
COPY --from=rust-builder --chown=10001:10001 /app/target/release/myurl-server /usr/local/bin/myurl-server
COPY --from=web-builder --chown=10001:10001 /app/apps/web/dist /app/web
USER 10001:10001
EXPOSE 3000
HEALTHCHECK --interval=5s --timeout=3s --retries=12 CMD ["curl", "--fail", "--silent", "http://127.0.0.1:3000/health/live"]
CMD ["/usr/local/bin/myurl-server"]
