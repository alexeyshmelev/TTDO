# syntax=docker/dockerfile:1
FROM python:3.13-slim-bullseye@sha256:e98b521460ee75bca92175c16247bdf7275637a8faaeb2bcfa19d879ae5c4b9a AS build
ARG ENDPOINT_DIR_NAME="TrustTunnel"
ARG RUST_DEFAULT_VERSION="1.95"
ARG RUSTUP_INSTALLER_SHA256="6c30b75a75b28a96fd913a037c8581b580080b6ee9b8169a3c0feb1af7fe8caf"
WORKDIR /home
# Install needed packets
RUN apt update && \
    apt install -y build-essential cmake curl make git libclang-dev
# Install Rust and Cargo
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh && \
    echo "${RUSTUP_INSTALLER_SHA256}  /tmp/rustup-init.sh" | sha256sum -c - && \
    sh /tmp/rustup-init.sh --default-toolchain "$RUST_DEFAULT_VERSION" -y && \
    rm /tmp/rustup-init.sh
ENV PATH="/root/.cargo/bin:$PATH"
# Copy source files
WORKDIR $ENDPOINT_DIR_NAME
COPY deeplink/ ./deeplink
COPY endpoint/ ./endpoint
COPY lib/ ./lib
COPY macros/ ./macros
COPY tools/ ./tools
COPY Cargo.toml Cargo.lock rust-toolchain.toml Makefile ./
# Build
RUN make endpoint/build
RUN make endpoint/build-wizard

# Copy binaries
FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818 AS trusttunnel-endpoint
ARG ENDPOINT_DIR_NAME="TrustTunnel"
ARG LOG_LEVEL="info"
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates iproute2 && rm -rf /var/lib/apt/lists/*
COPY --from=build /home/$ENDPOINT_DIR_NAME/target/release/setup_wizard /bin/
COPY --from=build /home/$ENDPOINT_DIR_NAME/target/release/trusttunnel_endpoint /bin/
COPY --chmod=755  /docker-entrypoint.sh /scripts/
WORKDIR /trusttunnel_endpoint

# Persist endpoint state/configuration under this directory:
# - vpn.toml
# - hosts.toml
# - credentials.toml
# - rules.toml
# - certs/
VOLUME /trusttunnel_endpoint/
ENTRYPOINT ["/scripts/docker-entrypoint.sh"]
