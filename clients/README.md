# TrustTunnel clients

This directory contains the client application and its native VPN engine in
the same repository as the TrustTunnel server.

```text
clients/
|-- app/                 Flutter user interface
|   |-- ios/             iOS host and packet-tunnel extension
|   |-- macos/           macOS host and packet-tunnel extension
|   `-- windows/         Windows host linked to the native engine
`-- engine/              C++, Rust, Swift, and Windows VPN implementation
```

The supported graphical clients are documented separately:

- [iOS source build](app/ios/README.md)
- [macOS source build](app/macos/README.md)
- [Windows source build](app/windows/README.md)

Read [the application guide](app/README.md) for the shared Flutter workflow
and [the engine guide](engine/README.md) for native architecture, command-line
builds, and tests. The exact imported revisions are recorded in
[the provenance file](PROVENANCE.md).

## Version namespaces

Server and client releases have independent version lines in this monorepo.
Endpoint releases use `vX.Y.Z` tags. Native client releases use
`client-vX.Y.Z` tags, and client build logic never derives a version from an
endpoint tag.

An explicit `TT_CLIENT_VERSION` value takes priority and must omit the
`client-v` tag prefix. Otherwise, the native build uses the nearest reachable
`client-v*` tag. A checkout without one reports `0.0.0-git` rather than
claiming the endpoint version or the imported upstream version. Record the
full source commit alongside any distributed artifact; a version string is
not a source-provenance substitute.

## Source and artifact policy

The repository contains source code, manifests, lock files, the Wintun API
header, and ordinary image assets. It does not contain compiled applications,
libraries, frameworks, drivers, or package archives. In particular:

- Apple XCFrameworks are built locally into
  `clients/engine/platform/apple/Framework/`.
- Flutter output is built locally below `clients/app/build/`.
- CMake and Conan output is built locally below
  `clients/engine/cmake-build-*` and the Conan cache.
- `wintun.dll` is not in the repository. Windows users supply the official,
  signed Wintun runtime after verifying its published checksum.

These paths are ignored by Git. Do not override the ignores to commit build
output. Before every commit, run `git status --short` from the repository root
and investigate any generated file that appears.

A source build is not an offline build. The first build downloads tools,
package recipes, and dependency source from the registries and source hosts
declared in the lock files and build manifests. Flutter also downloads its SDK
engine artifacts. After a successful first build, the relevant package-manager
caches can be preserved for later offline builds. Review
`clients/app/pubspec.lock`, `clients/engine/conanfile.py`, the Cargo lock files,
and `clients/engine/third-party/` before approving those dependencies.

## Runtime privacy boundary

The client has no analytics, crash-reporting service, remote log collector,
vendor account, advertising service, or update service. Its own network
destinations are limited to values in the configuration supplied by the
operator:

```text
client application
    |
    +-- configured TrustTunnel endpoint
    |
    +-- configured DNS upstream, when one is specified
    |
    `-- operating-system DNS, when no upstream is specified

traffic from other applications
    |
    `-- local TUN -> TrustTunnel endpoint -> destination requested by that app
```

There is no built-in public DNS fallback. Diagnostic logs remain on the device. The Apple **View Local Logs** action
creates local snapshots for the user to inspect; it does not upload them. The
Windows log-export bridge currently returns no files. Build tools are separate
from the finished application and may contact their own package registries or
signing services while resolving dependencies.

Disable optional build-tool analytics on a new workstation before building:

```bash
flutter config --no-analytics
dart --disable-analytics
```

Both Apple Podfiles also set `COCOAPODS_DISABLE_STATS=true`.

## Create and transfer a client configuration

The graphical application accepts TOML, not the `tt://` deep-link format. It
expands the endpoint's flat TOML export into the complete native TUN
configuration when connecting; an already-complete client configuration is
left unchanged. Export it on the installed VPS, where the live configuration
already exists; do not copy VPN credentials to the build VM:

```bash
sudo install -d -m 0700 /root/trusttunnel-client-configs
sudo sh -c '
cd /opt/trusttunnel
umask 077
./trusttunnel_endpoint vpn.toml hosts.toml \
    --client_config alice \
    --address vpn.example.invalid:443 \
    --format toml \
    > /root/trusttunnel-client-configs/alice.toml
'
```

Replace `alice` and `vpn.example.invalid:443` with the credential name and
public address of your endpoint. The hostname must match the TLS certificate.
Keep certificate verification enabled. If you configure a DNS upstream, choose
one that you operate or explicitly trust.

The resulting file contains a username and password. Never commit it, attach it
to an issue, put it in a GitHub release, or paste it into a chat. Transfer it
directly to the device over an authenticated channel, such as an encrypted USB
volume, AirDrop between your own Apple devices, or `scp` between machines you
control. Open the file locally, paste the entire export into the application's
configuration editor, and remove unnecessary copies.

## Dependency and platform references

- [Flutter platform setup](https://docs.flutter.dev/platform-integration)
- [Conan source-build modes](https://docs.conan.io/2/reference/commands/install.html#build-modes)
- [Apple packet-tunnel providers](https://developer.apple.com/documentation/networkextension/nepackettunnelprovider)
- [Official Wintun integration and build policy](https://git.zx2c4.com/wintun/about/)
