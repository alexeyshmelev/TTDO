# Privacy and Network Boundaries

This document states what a stock source build does at runtime, what it can
observe, and which optional actions contact services outside the deployment.
It is not a promise about a modified binary, operating system, package manager,
VPS provider, monitoring agent, or final traffic destination.

## Summary

The TrustTunnel endpoint and clients do not contain:

- analytics or advertising code;
- a remote crash-reporting client;
- a remote log-upload client;
- a vendor account or licensing connection;
- an automatic application updater;
- a hard-coded public VPN endpoint or public DNS fallback.

Diagnostic output stays in local files, callbacks, consoles, or the systemd
journal selected by the operator. TrustTunnel does not upload it.

The software is still a network forwarder. It must connect to the configured
endpoint and to destinations needed to carry user traffic. Those functional
connections are described below.

## Runtime destination inventory

### Endpoint process

| Destination | When it is used | Who selects it |
| --- | --- | --- |
| Client-requested TCP or UDP target | Normal direct forwarding | Application and client configuration |
| VPS operating-system DNS resolver | A requested target needs hostname resolution | VPS administrator |
| SOCKS5 proxy | `[forward_protocol.socks5]` is configured | Endpoint administrator |
| Reverse-proxy origin | `[reverse_proxy]` handles a configured host/path | Endpoint administrator |
| ICMP target | Optional ICMP forwarding is configured and requested | Application and endpoint policy |

The long-running endpoint has no vendor callback. It does not check GitHub for
updates, send health reports, push metrics, or obtain certificates by itself.

### Setup wizard

The setup wizard usually writes local TOML and certificate files. If the
operator selects Let's Encrypt issuance, the wizard intentionally connects to
the Let's Encrypt ACME directory and submits the account contact, hostname, and
certificate request needed for issuance. HTTP-01 also starts a temporary local
challenge listener on TCP port 80. The certificate private key remains local.

This is an explicit administrative action. Select an existing certificate or a
self-signed testing certificate if ACME network access is not desired.

The wizard does not remain running after setup and does not renew certificates
in the background.

### Client application and engine

| Destination | When it is used | Who selects it |
| --- | --- | --- |
| TrustTunnel endpoint | A VPN profile is connected | Endpoint configuration imported by the user |
| DNS upstream | The profile explicitly contains one | Operator or user |
| Operating-system DNS resolver | The profile has no explicit DNS upstream, or endpoint/encrypted-DNS hostnames need pre-resolution before tunnel changes | Device administrator |
| User traffic destinations | Applications use the local TUN or SOCKS listener | Local applications |

User traffic reaches its destination through the endpoint, so packet capture
on the client normally shows the endpoint connection rather than a separate
connection to each remote target. The endpoint makes those onward connections.

The app's **View Local Logs** function reads local diagnostic files. It does not
share or upload them. On Windows the Flutter bridge does not currently export
native log files.

## Build-time network access is separate

Reproducible source builds still require compilers and dependency source. On a
fresh workstation, build tools may contact:

- the Rust toolchain and Cargo sources declared by `rust-toolchain.toml`,
  `Cargo.toml`, and Cargo lock files;
- Flutter and Dart package sources declared by Flutter metadata,
  `pubspec.yaml`, and `pubspec.lock`;
- Conan remotes and source locations declared by client recipes and lock data;
- CocoaPods sources declared by the Apple Podfiles and the engine framework's
  reviewed lock file; application lock files are generated locally;
- Apple signing, provisioning, and notarization services when the builder
  chooses signed Apple distribution;
- GitHub when the operator clones source or transfers a self-built release
  asset.

These connections happen in development and packaging environments. They are
not services called by the installed endpoint or by an already built client.
Review every manifest, lock file, build script, and recipe before approving a
build. Preserve verified package-manager caches if an offline rebuild is
required.

Disable optional Flutter tooling analytics before resolving or compiling the
client:

```bash
flutter config --no-analytics
dart --disable-analytics
```

The client Podfiles disable CocoaPods statistics. Build-host telemetry from the
operating system, IDE, compiler installation, or third-party package manager is
outside TrustTunnel itself and should be configured separately.

## Data visible at each boundary

```text
local application
    |
    | application payload and destination
    v
client engine
    |
    | encrypted TrustTunnel connection; network metadata remains observable
    v
VPS endpoint
    |
    | requested destination and any payload not protected end-to-end
    v
destination or configured resolver/proxy
```

### Client device

The client needs access to the imported endpoint configuration, which can
contain a username, password, certificate, endpoint addresses, SNI, and DNS
settings. A system-wide tunnel also processes the device traffic selected by
its routing rules. Protect the device account and local application data.

### Network between client and VPS

TLS protects the TrustTunnel payload. An observer can still see transport
metadata such as endpoint IP address, ports, packet size and timing, connection
duration, and often DNS and SNI information from the surrounding deployment.
HTTP/3 can also be identified as QUIC traffic. TrustTunnel does not claim to
defeat all traffic analysis, active probing, endpoint blocking, or device
inspection.

### VPS endpoint

The endpoint terminates TrustTunnel TLS and authenticates the client. It must
know where to connect and can observe:

- the source address of the client connection;
- the authenticated account;
- the requested destination address and protocol;
- byte counts and connection timing;
- application bytes when the application does not use its own end-to-end
  encryption.

For HTTPS and other end-to-end encrypted application protocols, the endpoint
relays ciphertext for that inner protocol. For plain HTTP, plain DNS, and other
plaintext protocols, the endpoint can see content.

The VPS provider and host operating system can observe or alter anything the
endpoint can. Use a VPS and administrator account you trust, keep the host
patched, and restrict administrative access.

### Final destination

With direct forwarding, the destination normally sees the VPS address rather
than the client address. It receives the application protocol exactly as the
client initiated it. Destination-side logging, analytics, and privacy policies
are outside TrustTunnel.

## Credentials and client configurations

`credentials.toml` stores endpoint usernames and passwords. Exported TOML and
`tt://` values embed credentials for a selected user. Depending on certificate
type, the export can also contain certificate data.

Treat all of these as secrets:

- use a separate random password for each person or device where practical;
- store server files with restrictive ownership and mode;
- generate client files in a private directory with `umask 077`;
- transfer them through an authenticated encrypted channel;
- never put them in Git, a GitHub issue, CI logs, release assets, screenshots,
  or chat messages;
- revoke a leaked value by changing or removing it in `credentials.toml` and
  restarting the endpoint.

Non-interactive endpoint setup accepts `--creds-file`; non-interactive client
setup accepts `--creds-file`, `--endpoint_config`, and `--deeplink-file`.
These options put only a path in the process argument list. The wizards reject
symbolic links, non-regular files, oversized inputs, and, on Unix, files
accessible by group or other users. Secret output paths also reject existing
device nodes, pipes, sockets, and directories. The Docker entrypoint likewise
accepts `TT_CREDENTIALS_FILE`, which contains a path rather than the credential
value.

TLS client-random prefixes are admission signals, not substitutes for strong
credentials. Rules default to allow when no rule matches, so review the final
catch-all behavior deliberately.

An endpoint without a credentials file can behave as an unauthenticated proxy.
Never expose that configuration to the Internet. Keep
`allow_private_network_connections = false` unless reaching the VPS private
network is a deliberate and separately protected requirement.

## Logging

The endpoint logs to stdout unless `--logfile` is supplied. A supplied path
must resolve to a regular, non-symlink file; device nodes, sockets, pipes, and
directories are rejected before any truncation. Under systemd, stdout normally
enters the local journal. The client host decides whether native callbacks go
to a console or local file.

No TrustTunnel component transmits these logs. Nevertheless, logs are sensitive
local data. Depending on log level and failure conditions, they can contain IP
addresses, hostnames, connection identifiers, status codes, timing, and error
details. Use `info` for routine operation, restrict log readers, and set a
retention policy appropriate for the VPS.

Examples for systemd:

```bash
sudo journalctl --disk-usage
sudo journalctl -u trusttunnel --since today --no-pager
```

Do not collect debug or trace logs longer than needed. Review a log before
sharing even a small excerpt; remove credentials, client addresses,
destinations, and certificate details.

If a VPS image includes a separate monitoring, support, or log-shipping agent,
that agent is outside this repository. Disable or configure it at the operating
system level if third-party transmission is not acceptable.

## Metrics

Metrics are optional and pull-based. The default listener is
`127.0.0.1:1987`, which is reachable only from the VPS itself. It exposes
aggregate session counts, byte counts by HTTP transport, and outbound socket
counts; the endpoint does not push those values.

Keep the listener on loopback and let a trusted local scraper read it. If it is
bound to a public or private-network address, anyone who can reach that address
may query the unauthenticated HTTP endpoint. Use a firewall or authenticated
local reverse proxy if remote monitoring is required.

## Certificates

The endpoint private key is a server secret. A client configuration needs only
the public certificate when it cannot rely on a public trust store. Never copy
the private key to clients or package it with binaries.

Certificate renewal should replace the certificate and key atomically, then
send `SIGHUP` to reload `hosts.toml` and its referenced files. Confirm file
ownership after renewal. Avoid printing ACME account contacts or key material in
automation logs.

## Operational checklist

Before exposing a deployment:

- build from a reviewed commit and the locked dependency graph;
- verify the package checksum after transfer;
- keep client profiles, credentials, and private keys out of Git;
- require authentication and use unique random passwords;
- keep private-network forwarding disabled unless explicitly needed;
- bind metrics to loopback or leave metrics disabled;
- use a dedicated endpoint service account and least privilege;
- expose only TCP 443, optional UDP 443, and temporary TCP 80 for ACME;
- use `info` logs and configure local retention;
- document the VPS resolver, optional proxy, monitoring, and certificate
  authority as part of the deployment's own trust boundary;
- test update and rollback without replacing configuration.

For concrete commands, continue with the
[source-build deployment guide](SOURCE_BUILDS.md).
