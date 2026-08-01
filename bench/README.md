# Architecture

The benchmark consists of 3 isolated parts:

- `remote-side` - acts as HTTP servers for the benchmark
- `middle-box` - acts as a VPN endpoint host, either WireGuard or TrustTunnel
- `local-side` - acts as a benchmark running host, can establish tunnels to the server
  residing on the remote side through the VPN endpoint

## How to run

### Single host

1) Build docker images

   ```shell
   cd ./bench
   ./single_host.sh build
   ```

   The benchmark builds the endpoint and client engine from this monorepo
   checkout. It does not clone a second client repository.

   To see the full set of available options run:

   ```shell
   ./single_host.sh --help
   ```

2) Run the benchmark

   ```shell
   ./single_host.sh run
   ```

   This command runs all the parts of the benchmark on the current host.

### Separate hosts

Assume IP addresses of `host_1`, `host_2` and `host_3` are
`192.0.2.10`, `198.51.100.20`, and `203.0.113.30` respectively. These
documentation addresses must be replaced with the real addresses of your
benchmark hosts.

1) Running `host_1` as a remote side

   ```shell
   ssh user@192.0.2.10
   git clone <TrustTunnel-monorepo.git> ~/TrustTunnel
   cd ~/TrustTunnel
   docker build -t bench-rs bench/remote-side
   docker run -d -p 8080:8080 -p 5201:5201 -p 5201:5201/udp bench-rs
   ```

2) Running `host_2` as a middle box

   The endpoint is built from the repository root. Clone the project on the
   middle-box host:

   ```shell
   ssh user@198.51.100.20
   git clone <TrustTunnel-monorepo.git> ~/TrustTunnel
   cd ~/TrustTunnel
   docker build -t bench-common bench
   ```

    - WireGuard

       ```shell
       docker build -t bench-mb-wg bench/middle-box/wireguard
       docker run -d \
         --cap-add=NET_ADMIN --cap-add=SYS_MODULE --device=/dev/net/tun \
         -p 51820:51820/udp \
         bench-mb-wg
       ```

    - TrustTunnel

       ```shell
       docker build \
         --build-arg ENDPOINT_HOSTNAME=endpoint.bench \
         -f bench/middle-box/trusttunnel-rust/Dockerfile \
         -t bench-mb-ag .
       docker run -d \
         -p 4433:4433 -p 4433:4433/udp \
         bench-mb-ag
       ```

3) Run the benchmark from `host_3`

   ```shell
   ssh user@203.0.113.30
   git clone <TrustTunnel-monorepo.git> ~/TrustTunnel
   cd ~/TrustTunnel
   docker build -t bench-common bench
   docker build -t bench-ls bench/local-side
   ```

   - No VPN

      ```shell
      ./bench/local-side/bench.sh no-vpn bridge 192.0.2.10 results/no-vpn
      ```

   - WireGuard

      ```shell
      docker build -t bench-ls-wg bench/local-side/wireguard
      ./bench/local-side/bench.sh wg bridge 192.0.2.10 results/wg 198.51.100.20
      ```

   - TrustTunnel

      ```shell
      docker build \
        -f bench/local-side/trusttunnel/Dockerfile \
        -t bench-ls-ag .
      ./bench/local-side/bench.sh ag bridge 192.0.2.10 results/ag \
        198.51.100.20 endpoint.bench
      ```
