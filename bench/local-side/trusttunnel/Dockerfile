# syntax=docker/dockerfile:1
FROM bench-ls

ARG CONAN_VER=2.31.1
ARG GOLANG_VER=1.25.4
ARG LLVM_MAJOR_VER=21

ARG NLC_URL="https://github.com/AdguardTeam/NativeLibsCommon.git"
ARG DNS_LIBS_URL="https://github.com/AdguardTeam/DnsLibs.git"

# Install Go for source-built client dependencies.
RUN arch=$(arch | sed s/aarch64/arm64/ | sed s/x86_64/amd64/) && \
    case "$arch" in \
        amd64) go_sha256="9fa5ffeda4170de60f67f3aa0f824e426421ba724c21e133c1e35d6159ca1bec" ;; \
        arm64) go_sha256="a68e86d4b72c2c2fecf7dfed667680b6c2a071221bbdb6913cf83ce3f80d9ff0" ;; \
        *) echo "Unsupported architecture: $arch"; exit 1 ;; \
    esac && \
    apt install -y iptables ninja-build && \
    pip install "conan==$CONAN_VER" && \
    curl -fsSL https://dl.google.com/go/go${GOLANG_VER}.linux-${arch}.tar.gz -o /tmp/go.tar.gz && \
    echo "${go_sha256}  /tmp/go.tar.gz" | sha256sum -c - && \
    rm -rf /usr/local/go && tar -C /usr/local -xzf /tmp/go.tar.gz && rm /tmp/go.tar.gz
ENV PATH="/usr/lib/llvm-$LLVM_MAJOR_VER/bin:/usr/local/go/bin:$PATH"

COPY rust-toolchain.toml /bench/TrustTunnel/rust-toolchain.toml
COPY deeplink /bench/TrustTunnel/deeplink
COPY clients/engine /bench/TrustTunnel/clients/engine
WORKDIR /bench/TrustTunnel/clients/engine
RUN --mount=type=cache,target=/root/.conan2 \
    python3 -m venv env && \
    . env/bin/activate && \
    pip install -r requirements.txt && \
    conan profile detect --force && \
    ./scripts/bootstrap_conan_deps.py "$NLC_URL" "$DNS_LIBS_URL"
RUN --mount=type=cache,target=/root/.conan2 \
    --mount=type=cache,target=/root/.cargo/registry \
    . env/bin/activate && \
    SKIP_BOOTSTRAP=1 make setup_cmake

# fixme: for some reason certificate CN check fails with non-rust endpoint
COPY bench/local-side/trusttunnel/ssl_verify.patch /bench/

WORKDIR /bench/TrustTunnel/clients/engine
RUN --mount=type=cache,target=/root/.conan2 \
    patch -p1 < /bench/ssl_verify.patch && \
    SKIP_BOOTSTRAP=1 make build_trusttunnel_client && \
    cp cmake-build-clang-relwithdebinfo/trusttunnel/trusttunnel_client /bench/

COPY bench/local-side/trusttunnel/entrypoint.sh /bench/

WORKDIR /bench/
ENTRYPOINT ["./entrypoint.sh"]
CMD ["endpoint_hostname", "endpoint_ip", "protocol", "mode", "socks_port_first", "socks_port_last"]
