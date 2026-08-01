# TrustTunnel

TrustTunnel is a source-first VPN endpoint and client monorepo. It contains the
Linux server, configuration tools, protocol libraries, and graphical client
source for iOS, macOS, and Windows.

TrustTunnel carries application traffic through TLS using HTTP/1.1, HTTP/2, or
HTTP/3. This makes the tunnel resemble ordinary HTTPS at the protocol level,
but it does not make traffic unobservable and cannot guarantee that a network
operator will be unable to identify or block it.

This repository does not provide a hosted VPN service. You supply a VPS, a
hostname and certificate, and client credentials.

## Repository map

```text
TrustTunnel/
|-- endpoint/              Linux endpoint executable
|-- lib/                   Server protocol and forwarding library
|-- tools/                 Endpoint setup wizard
|-- deeplink/              tt:// configuration encoder and decoder
|-- macros/                Rust procedural macros
|-- clients/
|   |-- app/               Flutter app for iOS, macOS, and Windows
|   `-- engine/            Native C++, Rust, Swift, and Windows client engine
|-- docs/                  Architecture, privacy, and source-build guides
`-- scripts/               Service and development helpers
```

Start with these guides:

- [Architecture and terminology](docs/ARCHITECTURE.md)
- [Runtime privacy and network boundaries](docs/PRIVACY.md)
- [Build on a PC and deploy to a small VPS](docs/SOURCE_BUILDS.md)
- [Endpoint configuration reference](CONFIGURATION.md)
- [Client build guides](clients/README.md)
- [Client source provenance](clients/PROVENANCE.md)
- [Third-party notices](THIRD_PARTY_NOTICES.md)
- [Protocol specification](PROTOCOL.md)

## How a connection works

The client, not the VPS, owns the virtual network interface:

```text
application on phone or PC
          |
          v
client TUN interface or local SOCKS listener
          |
          v
TrustTunnel client engine
          |
          | TLS carrying HTTP/1.1, HTTP/2, or HTTP/3
          v
TCP 443 or UDP 443 on the VPS
          |
          v
TrustTunnel endpoint: authenticate, decode, apply policy
          |
          v
ordinary outbound TCP/UDP socket, or optional raw ICMP socket
          |
          v
destination requested by the application
```

The endpoint is an application-level forwarder. A normal native deployment
does **not** need a server-side TUN device, Linux IP forwarding,
`MASQUERADE`, or other NAT rules. Those are common requirements for routed VPNs
such as WireGuard, but they are not part of TrustTunnel's direct forwarder.
Docker still uses its normal bridge and port-publishing rules.

The TrustTunnel TLS session ends on the VPS. Traffic from the VPS to the final
destination is protected only by the destination protocol. For example, an
HTTPS request remains protected by the application's own HTTPS connection;
plain HTTP, plain DNS, and other unencrypted protocols are plaintext after
leaving the endpoint.

## Source and privacy policy

The Git tree contains source, lock files, build descriptions, licenses, and
ordinary media assets. Compiled executables, libraries, frameworks, drivers,
and package archives do not belong in the repository. Build output stays in
ignored directories.

A source-only repository is not the same as an offline build. Cargo, Flutter,
Conan, CocoaPods, and platform toolchains may fetch compiler components,
package metadata, and dependency source during a build. Those build-time
destinations are declared by manifests and lock files; they are not runtime
destinations embedded in the finished server or client.

The endpoint and clients contain no analytics, remote crash reporter, remote
log collector, advertising SDK, or automatic updater. Logs remain on the
machine where the process runs. The endpoint can still make intentional
network connections for its job: it forwards client-requested traffic, may
resolve requested hostnames with the VPS resolver, may contact a configured
SOCKS or reverse-proxy target, and the setup wizard can contact an ACME
certificate authority when the operator asks it to issue a certificate. See
[the privacy guide](docs/PRIVACY.md) for the complete boundary.

## Server requirements

For a typical native Ubuntu deployment you need:

- a Linux VPS with a public IPv4 address;
- a DNS `A` record such as `vpn.example.com` pointing to that address;
- inbound TCP port 443;
- inbound UDP port 443 if HTTP/3 is enabled;
- outbound TCP and UDP access to destinations clients are allowed to reach;
- a TLS certificate whose name matches the hostname;
- a build machine with substantially more memory than a 512 MB VPS.

Port 80 is needed only while using the setup wizard's optional ACME HTTP-01
flow. If the VPS has no working outbound IPv6 route, do not publish an `AAAA`
record and set `ipv6_available = false`.

## Build the server from source

Building the endpoint compiles Rust and native TLS/QUIC dependencies. On a
512 MB VPS this is slow and may be killed by the out-of-memory manager. Build
on an Ubuntu VM on your PC, then copy the two executables to the VPS.

Use the same CPU architecture and, preferably, the same Ubuntu release as the
VPS. Compare these on both machines:

```bash
dpkg --print-architecture
ldd --version | head -n 1
```

On the build VM, install the compiler prerequisites:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends \
    build-essential ca-certificates clang cmake curl git libclang-dev make \
    pkg-config python3
```

Install Rust with the official Rust installer, inspect this repository, and
build the exact locked dependency graph:

```bash
curl --proto '=https' --tlsv1.2 --fail --show-error \
    https://sh.rustup.rs -o /tmp/rustup-init.sh
sh /tmp/rustup-init.sh -y --profile minimal
. "$HOME/.cargo/env"

git clone <your-repository-url> TrustTunnel
cd TrustTunnel
git checkout <commit-or-tag>
test -z "$(git status --porcelain --untracked-files=normal)" || {
    echo "Refusing to build from a dirty source tree" >&2
    exit 1
}
rustup component add clippy rustfmt
rustup show active-toolchain
cargo build --locked --release \
    --bin trusttunnel_endpoint --bin setup_wizard
```

The pinned toolchain is defined by `rust-toolchain.toml`. Before deploying,
run the server checks on the build VM:

```bash
make lint-rust
make test
```

The outputs are:

```text
target/release/trusttunnel_endpoint
target/release/setup_wizard
```

Do not commit these files. Package them outside the source tree and record a
checksum:

```bash
package_dir="$(mktemp -d)"
install -d "$package_dir/trusttunnel"
install -m 0755 target/release/trusttunnel_endpoint \
    "$package_dir/trusttunnel/"
install -m 0755 target/release/setup_wizard \
    "$package_dir/trusttunnel/"
install -m 0644 scripts/trusttunnel.service.template \
    "$package_dir/trusttunnel/"
install -m 0644 LICENSE THIRD_PARTY_NOTICES.md \
    "$package_dir/trusttunnel/"
git rev-parse HEAD > "$package_dir/trusttunnel/SOURCE_COMMIT"
rustc --version --verbose > "$package_dir/trusttunnel/RUSTC_VERSION"
(
    . /etc/os-release
    printf 'ubuntu_version=%s\n' "$VERSION_ID"
    printf 'dpkg_architecture=%s\n' "$(dpkg --print-architecture)"
    uname -m
    ldd --version | head -n 1
) > "$package_dir/trusttunnel/TARGET_INFO"
cargo install --locked --version 0.9.1 --features cli cargo-about
cargo about generate --locked --offline --workspace --fail \
    --config scripts/licenses/about.toml \
    --output-file "$package_dir/trusttunnel/THIRD_PARTY_LICENSES.html" \
    scripts/licenses/about.hbs
(
    cd "$package_dir/trusttunnel"
    sha256sum trusttunnel_endpoint setup_wizard > BINARY_SHA256SUMS
)
tar --owner=0 --group=0 --numeric-owner \
    -C "$package_dir" -czf ../trusttunnel-server-linux.tar.gz trusttunnel
(cd .. && sha256sum trusttunnel-server-linux.tar.gz \
    > trusttunnel-server-linux.tar.gz.sha256)
```

Use `scp` for the simplest private transfer. GitHub Release assets are another
option when you need GitHub as the transfer point; release assets are outside
the Git history and must never include configuration, credentials,
certificates, or private keys. Both procedures, including checksum verification
and rollback, are in the [source-build deployment guide](docs/SOURCE_BUILDS.md).

### Automated container setup

For a non-interactive first start, give the container a protected credentials
file rather than a credential environment value. Create a root-only file whose
only line is `username:password`; using an editor avoids putting that value in
shell history:

```bash
sudo install -d -m 0700 /srv/trusttunnel
sudo install -m 0600 /dev/null \
    /srv/trusttunnel/bootstrap.credentials
sudoedit /srv/trusttunnel/bootstrap.credentials
docker build -t trusttunnel-endpoint .
docker run -d --name trusttunnel --restart unless-stopped \
    -p 443:8443/tcp -p 443:8443/udp \
    -e TT_HOSTNAME=vpn.example.com \
    -e TT_CREDENTIALS_FILE=/run/secrets/trusttunnel_credentials \
    --mount type=bind,src=/srv/trusttunnel,dst=/trusttunnel_endpoint \
    --mount type=bind,src=/srv/trusttunnel/bootstrap.credentials,dst=/run/secrets/trusttunnel_credentials,readonly \
    trusttunnel-endpoint
```

When `TT_CREDENTIALS_FILE` is omitted, the entrypoint uses
`/run/secrets/trusttunnel_credentials`. The file must be a regular,
non-symbolic-link file and, on Unix, must not grant group or other access. The
entrypoint passes only its path to `setup_wizard`; it never copies the secret
into an argument or environment value. Keep the generated files in
`/srv/trusttunnel` protected and backed up. Automatic setup runs only when
`credentials.toml`, `vpn.toml`, `hosts.toml`, `rules.toml`,
`certs/cert.pem`, and `certs/key.pem` are all absent. If a volume contains any
residual wizard output but not all three primary TOML files, startup stops
without changing anything; restore the missing files from backup or
deliberately move the whole prior setup aside before starting fresh. For
`TT_CERT_TYPE=provided`, mount the read-only source certificate and key outside
`/trusttunnel_endpoint`, such as under `/run/secrets`, and point
`TT_CERT_PROVIDED_CHAIN_PATH` and `TT_CERT_PROVIDED_KEY_PATH` there.

## First VPS installation

If this VPS already runs TrustTunnel from a prebuilt package, do not treat it
as a fresh installation. Inventory and back up the existing unit, executable,
configuration, and certificates, then stop its listener by following the
[package-migration procedure](docs/SOURCE_BUILDS.md#migrate-an-existing-packaged-installation).

Copy the archive and checksum to the VPS, verify them, and install the files:

```bash
sha256sum -c trusttunnel-server-linux.tar.gz.sha256
if ! getent group trusttunnel >/dev/null 2>&1; then
    sudo groupadd --system trusttunnel
fi
if ! id -u trusttunnel >/dev/null 2>&1; then
    sudo useradd --system --home-dir /opt/trusttunnel \
        --gid trusttunnel --shell /usr/sbin/nologin trusttunnel
fi
sudo install -d -o root -g trusttunnel -m 0750 /opt/trusttunnel
sudo tar --no-same-owner -xzf trusttunnel-server-linux.tar.gz \
    -C /opt/trusttunnel --strip-components=1
sudo chown root:trusttunnel /opt/trusttunnel
sudo chmod 0750 /opt/trusttunnel
sudo chown root:root \
    /opt/trusttunnel/trusttunnel_endpoint \
    /opt/trusttunnel/setup_wizard
sudo chmod 0755 \
    /opt/trusttunnel/trusttunnel_endpoint \
    /opt/trusttunnel/setup_wizard
sudo sh -c '
cd /opt/trusttunnel
sha256sum -c BINARY_SHA256SUMS
printf "Source commit: "
cat SOURCE_COMMIT
'
```

Compare the full source commit with the commit or tag reviewed on the build VM.
Then run the interactive wizard from the protected installation directory:

```bash
sudo sh -c 'cd /opt/trusttunnel && exec ./setup_wizard'
```

The wizard creates these local files:

- `vpn.toml`: listener, forwarding, timeout, and optional metrics settings;
- `hosts.toml`: TLS names and certificate paths;
- `credentials.toml`: client usernames and passwords;
- `rules.toml`: optional connection-admission rules;
- `certs/`: certificate and private-key files when the wizard creates them.

Use a long random password. Keep all of these files, especially
`credentials.toml` and private keys, out of GitHub and release archives. Restrict
their permissions after setup:

```bash
sudo sh -c '
set -eu
cd /opt/trusttunnel
for file in vpn.toml hosts.toml credentials.toml rules.toml; do
    if [ -f "$file" ]; then
        chown root:trusttunnel "$file"
        chmod 0640 "$file"
    fi
done
if [ -d certs ]; then
    find certs -type d -exec chown root:trusttunnel {} +
    find certs -type d -exec chmod 0750 {} +
    find certs -type f -exec chown root:trusttunnel {} +
    find certs -type f -exec chmod 0640 {} +
fi
'
```

The wizard offers three certificate paths:

- use an existing certificate and private key;
- ask an ACME certificate authority to issue one;
- create a self-signed certificate for controlled testing.

A publicly trusted certificate is the easiest choice for the graphical
clients. ACME is an explicit setup-time network action, not telemetry. HTTP-01
requires the hostname to resolve to the VPS and TCP port 80 to reach the
wizard while validation runs. Certificate renewal remains an operator
responsibility; see [certificate renewal](CERT_RENEWAL.md).

## Firewall and cloud network

Preserve SSH access before enabling a host firewall. For UFW:

```bash
sudo sshd -T | awk '$1 == "port" {print $2}'
sudo ufw allow OpenSSH
```

The `OpenSSH` profile normally covers only TCP port 22. If `sshd -T` prints a
different port, allow every actual SSH port before enabling UFW; replace 2222
below with the real value:

```bash
sudo ufw allow 2222/tcp
```

Then add the TrustTunnel rules and enable UFW:

```bash
sudo ufw allow 443/tcp
sudo ufw allow 443/udp
sudo ufw enable
sudo ufw status verbose
```

Keep the current SSH session open and confirm a second SSH login succeeds after
enabling UFW. Add the same SSH and TrustTunnel ports to the provider firewall
before closing the working session.

Omit the UDP rule if `[listen_protocols.quic]` is disabled. Temporarily allow
`80/tcp` only when using ACME HTTP-01:

```bash
sudo ufw allow 80/tcp
```

Apply matching rules in the VPS provider's firewall. A provider firewall and
UFW are separate layers. The VPS also needs outbound access for the destinations
the endpoint is expected to reach.

Do not add `net.ipv4.ip_forward=1`, a server TUN device, or NAT rules for a
native direct-forwarder deployment. Leave the optional `[icmp]` section off
during initial setup; ICMP needs a raw socket and an additional Linux
capability, but TCP and UDP forwarding do not.

## Run with systemd

Review the bundled service template and adjust its paths if you did not use
`/opt/trusttunnel`. Install it, then start the service:

```bash
sudo install -m 0644 /opt/trusttunnel/trusttunnel.service.template \
    /etc/systemd/system/trusttunnel.service
sudo systemctl daemon-reload
sudo systemctl enable --now trusttunnel
sudo systemctl status trusttunnel --no-pager
```

Run an Internet-facing endpoint as a dedicated service account with only the
capabilities it needs. `CAP_NET_BIND_SERVICE` permits a non-root service to bind
port 443. Add `CAP_NET_RAW` only when the optional ICMP forwarder is enabled.
The detailed unit setup is in the
[source-build deployment guide](docs/SOURCE_BUILDS.md#install-and-run-on-the-vps).

The endpoint writes to stdout by default, so systemd stores its local logs in
the journal:

```bash
sudo journalctl -u trusttunnel -b -n 100 --no-pager
sudo journalctl -u trusttunnel -f
```

Use `info` logging for normal operation. Debug and trace logs are more verbose
and may expose additional connection metadata. Nothing uploads the journal or
log files; retention and access are controlled by the VPS administrator.

## Export a client configuration

Generate a configuration only after the public address, certificate, and
`ipv6_available` setting are correct. The selected username must exist in
`credentials.toml`.

The graphical app accepts the endpoint's flat TOML export and expands it into
a complete native TUN configuration locally. Create it in a private directory:

```bash
sudo install -d -m 0700 /root/trusttunnel-client-configs
sudo sh -c '
cd /opt/trusttunnel
umask 077
./trusttunnel_endpoint vpn.toml hosts.toml \
    --client_config alice --address vpn.example.com:443 --format toml \
    > /root/trusttunnel-client-configs/alice.toml
'
```

The default output format is a `tt://?` deep link:

```bash
sudo sh -c 'cd /opt/trusttunnel && exec ./trusttunnel_endpoint \
    vpn.toml hosts.toml --client_config alice \
    --address vpn.example.com:443'
```

The endpoint prints the value locally. It does not send it to an external QR
or configuration website. Both formats contain credentials; transfer them only
through a channel you control and never commit them to this repository.

Build and install a client by following [the client guides](clients/README.md).

## Routine operation

Check the process, sockets, logs, DNS, and TLS certificate with:

```bash
sudo systemctl status trusttunnel --no-pager
sudo ss -lntup '( sport = :443 )'
getent ahosts vpn.example.com
openssl s_client -connect vpn.example.com:443 \
    -servername vpn.example.com -verify_hostname vpn.example.com \
    -verify_return_error -alpn h2 </dev/null
```

The TLS check should report `Verify return code: 0 (ok)` and select `h2` when
HTTP/2 is enabled.

Only `hosts.toml` and the certificate files it references can be reloaded
without restarting. After replacing them atomically, signal the main service
process:

```bash
sudo systemctl kill --kill-who=main --signal=HUP trusttunnel
sudo journalctl -u trusttunnel -n 20 --no-pager
```

Restart after changing `vpn.toml`, `credentials.toml`, or `rules.toml`:

```bash
sudo systemctl restart trusttunnel
```

## Troubleshooting

### TCP connects but HTTP/3 does not

HTTP/1.1 and HTTP/2 use TCP 443. HTTP/3 uses UDP 443. Check UFW, the cloud
firewall, and `ss -lnup` independently. Begin with HTTP/2, then enable QUIC.

### TLS verification fails

Confirm that the exported client address, TLS SNI, certificate subject names,
DNS record, and `hosts.toml` entry describe the same hostname. Check the full
certificate chain and system clock. An IP address cannot validate against a
certificate that contains only a DNS name unless a matching SNI override is
configured deliberately.

### IPv4 works but some requests stall

Set `ipv6_available = false` unless the VPS has a working outbound IPv6 route.
Remove an unusable `AAAA` record, restart the endpoint, and export/import a new
client configuration because the capability is embedded in that configuration.

### The endpoint is running but sites do not open

Check outbound provider-firewall rules, VPS DNS resolution, and whether
`allow_private_network_connections = false` is rejecting a private destination.
Test the VPS's own outbound TCP and UDP access. TrustTunnel does not create an
Internet route that the VPS itself lacks.

### The process is killed on a 512 MB VPS

Build elsewhere. At runtime, use `--jobs 1`, start with HTTP/2 only, leave ICMP
off, and inspect both service and kernel logs:

```bash
sudo journalctl -u trusttunnel -b --no-pager
sudo journalctl -k -b --no-pager | grep -i -E 'oom|killed process'
```

### A configuration change had no effect

Only TLS host settings reload on `SIGHUP`. Restart for all other configuration
changes. Verify that the service `WorkingDirectory` makes relative file paths
resolve to the intended files.

## Updating and rollback

Build every update from a reviewed commit on the compatible Ubuntu VM, run the
tests, checksum the package, and stage its new executables and provenance
metadata, licenses, and service template. Keep the previous package files until
the new version has started and passed a client connection test. Review any
service-template diff separately, and do not overwrite configuration or
certificate files as part of a binary update.

Verify the installed binary checksums and identify the source revision with:

```bash
sudo sh -c '
cd /opt/trusttunnel
sha256sum -c BINARY_SHA256SUMS
cat SOURCE_COMMIT
'
```

Compare `SOURCE_COMMIT` with the reviewed release tag or commit. The endpoint's
`--version` output reports the server software version and may still say
`1.0.41` for this fork; it does not identify which repository changes were
built. Endpoint `v*` tags are independent of native client `client-v*` tags.

The [source-build guide](docs/SOURCE_BUILDS.md#update-and-roll-back) gives an
explicit stopped-service `.new`/rollback-directory procedure and the matching
rollback commands. The two executable moves are sequential while the service
is stopped, not an atomic pair replacement.

## Development checks

Server changes must pass:

```bash
make lint-rust
make lint-md
make test
```

Client prerequisites and checks are platform-specific; see
[clients/README.md](clients/README.md). Generated binaries and private runtime
configuration must remain untracked.

## License

TrustTunnel is licensed under the Apache License 2.0. See [LICENSE](LICENSE)
and the repository's third-party notices for dependency licenses.
