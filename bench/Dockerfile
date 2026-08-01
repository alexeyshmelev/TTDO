# syntax=docker/dockerfile:1
FROM python:3.13-slim-bullseye@sha256:e98b521460ee75bca92175c16247bdf7275637a8faaeb2bcfa19d879ae5c4b9a

ARG RUST_CHANNEL=1.95
ARG LLVM_MAJOR_VER=21
ARG LLVM_INSTALLER_SHA256=9474ecd78b52aba6e923976b1e9773f5613027cc7e237b9956986cb536e02a36
ARG RUSTUP_INSTALLER_SHA256=6c30b75a75b28a96fd913a037c8581b580080b6ee9b8169a3c0feb1af7fe8caf

# Install the required utilities
RUN apt update && \
    apt install -y build-essential curl git gnupg lsb-release iproute2 \
    net-tools software-properties-common wget tar libssl-dev xz-utils

# Install pre-built CMake
RUN set -ex; \
    CMAKE_VERSION="3.31.10"; \
    ARCH=$(uname -m); \
    case "$ARCH" in \
        x86_64)  CMAKE_ARCH="x86_64"; CMAKE_SHA256="3cb3dd247b6a1de2d0f4b20c6fd4326c9024e894cebc9dc8699758887e566ca7" ;; \
        aarch64) CMAKE_ARCH="aarch64"; CMAKE_SHA256="a343c6294f770742904e6a6792e0956b5ff8212abfb63cac99237de2e210fa0f" ;; \
        *) echo "Unsupported architecture: $ARCH"; exit 1 ;; \
    esac; \
    curl -L "https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-${CMAKE_ARCH}.tar.gz" \
        -o /tmp/cmake.tar.gz && \
    echo "${CMAKE_SHA256}  /tmp/cmake.tar.gz" | sha256sum -c - && \
    tar -xzf /tmp/cmake.tar.gz -C /usr/local --strip-components=1 && \
    rm /tmp/cmake.tar.gz

# Install pre-built static curl with HTTP/3 support
RUN set -ex; \
    ARCH=$(uname -m); \
    case "$ARCH" in \
        x86_64)  CURL_ARCH="x86_64"; CURL_SHA256="5e5703d2862e0cadeae3213597106a7f96efb074261278a50cf02a5bfd0c7ed4" ;; \
        aarch64) CURL_ARCH="aarch64"; CURL_SHA256="a6cd84aa62c75329062bc8ac0dfdbabe59b28ba72d42787b763f0d2fd4fdb692" ;; \
        *) echo "Unsupported architecture: $ARCH"; exit 1 ;; \
    esac; \
    CURL_VERSION="8.18.0"; \
    curl -L "https://github.com/stunnel/static-curl/releases/download/${CURL_VERSION}/curl-linux-${CURL_ARCH}-glibc-${CURL_VERSION}.tar.xz" \
        -o /tmp/curl.tar.xz && \
    echo "${CURL_SHA256}  /tmp/curl.tar.xz" | sha256sum -c - && \
    tar -xf /tmp/curl.tar.xz -C /usr/local/bin && \
    rm /tmp/curl.tar.xz && \
    chmod +x /usr/local/bin/curl

# Install LLVM.
RUN curl -fsSL https://apt.llvm.org/llvm.sh -o /tmp/llvm.sh && \
    echo "${LLVM_INSTALLER_SHA256}  /tmp/llvm.sh" | sha256sum -c - && \
    chmod +x /tmp/llvm.sh && \
    /tmp/llvm.sh "$LLVM_MAJOR_VER" && \
    rm /tmp/llvm.sh && \
    apt install -y libc++-${LLVM_MAJOR_VER}-dev && \
    apt install -y libclang-${LLVM_MAJOR_VER}-dev && \
    ln -s libc++abi.so.1 /usr/lib/llvm-$LLVM_MAJOR_VER/lib/libc++abi.so
# Install Rust and Cargo.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh && \
    echo "${RUSTUP_INSTALLER_SHA256}  /tmp/rustup-init.sh" | sha256sum -c - && \
    sh /tmp/rustup-init.sh -y --default-toolchain "${RUST_CHANNEL}" && \
    rm /tmp/rustup-init.sh
ENV PATH="/root/.cargo/bin:${PATH}"

CMD bash
