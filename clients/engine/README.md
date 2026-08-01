# TrustTunnel native client engine

The engine implements the TrustTunnel protocol, TUN and SOCKS listeners, DNS
handling, route management, and the native platform adapters used by the
graphical client. It supports HTTP/1.1, HTTP/2, and HTTP/3 transports and can
tunnel TCP, UDP, and ICMP traffic.

## Layout

```text
common/                 shared event-loop, settings, logging, and utilities
net/                    sockets, TLS, QUIC, DNS, pinger, and OS tunnel code
tcpip/                  vendored lwIP integration and packet processing
core/                   VPN session, routing, listeners, and upstream logic
trusttunnel/            command-line client and Rust configuration tools
platform/apple/         Swift adapter and source-built XCFramework scripts
platform/windows/       C++ adapter linked by the Flutter Windows host
third-party/            source snapshots, licenses, and API headers
```

The dependency direction is:

```text
common <- net <- core <- command-line client
   ^        ^       ^
   |        |       |
   +----- tcpip ----+
                    |
          Apple and Windows adapters
```

## Source-only boundary

No compiled engine library, XCFramework, executable, driver, or dependency
archive belongs in this directory. CMake, Conan, Cargo, and Xcode create all
native output locally. The vendored Conan integration and recipes are pinned;
the default dependency mode builds required C and C++ packages from source.
Conan and Cargo still download package recipes and source archives on the first
build. CMake, Ninja, compilers, SDKs, and other host build tools come from the
documented workstation prerequisites; the Conan lock deliberately does not
inject binary-distributed build tools.

Windows is the one runtime exception outside the repository. TUN mode needs the
official signed `wintun.dll`, installed by the operator after checksum
verification. The repository contains only Wintun's API header and license.
See the [Windows application guide](../app/windows/README.md).

## Prerequisites

- Python 3.13 or newer
- CMake 3.24 or newer
- Conan 2.31.1, pinned in `requirements.txt`
- Ninja 1.13 or newer
- Rust 1.95, selected by the repository-root
  [`rust-toolchain.toml`](../../rust-toolchain.toml)
- A C++20 compiler; repository linting requires LLVM and clang-format 21 or
  newer, while Windows builds use Visual Studio 2022 MSVC
- Windows builds also require Strawberry Perl, NASM, and a Windows SDK
- Linux builds require `libc++-dev` and `libc++abi-dev`; linting additionally
  requires `jq` and `markdownlint-cli`
- Apple framework builds require Xcode, CocoaPods, and the iOS platform SDK

Create an ignored Python environment with the pinned build tools, then create
the default Conan profile once on each workstation:

```bash
python3 -m venv env
. env/bin/activate
python -m pip install --requirement requirements.txt
conan profile detect --force
```

## Build the command-line client

On macOS or Linux, from `clients/engine`:

```bash
SKIP_VENV=1 make bootstrap_deps
make build_trusttunnel_client
make build_wizard
```

The release outputs are
`cmake-build-clang-relwithdebinfo/trusttunnel/trusttunnel_client` and
`cmake-build-clang-relwithdebinfo/trusttunnel/setup_wizard`.

On Windows, run the following in **Developer PowerShell for VS 2022** from
`clients\engine`. These direct CMake commands use the checked-in MSVC preset;
GNU Make is not a Windows prerequisite:

```powershell
python -m pip install --requirement .\requirements.txt
conan profile detect --force
python .\scripts\bootstrap_conan_deps.py
cmake --preset msvc-relwithdebinfo
cmake --build .\cmake-build-msvc-relwithdebinfo `
    --target trusttunnel_client setup_wizard
```

Build outputs stay in `cmake-build-*`; never copy them into the source tree.
The command-line client needs elevated privileges for a system TUN, although a
loopback SOCKS listener can be used without installing a TUN driver.

## Client versioning

Client builds resolve their version independently of the server. An explicit
`TT_CLIENT_VERSION` value is used first, followed by the nearest reachable
`client-v*` tag. The environment value is the version itself, without the
`client-v` prefix. If neither source exists, the build reports `0.0.0-git`.
Generic `v*` tags belong to endpoint releases and are deliberately ignored.

Conan export, the command-line client, the setup wizard, Windows version
resources, CocoaPods, and Apple framework metadata all use this policy. The
full Git commit remains the authoritative identity for an untagged or modified
source build.

## Build platform adapters

- [Apple source build](../app/ios/README.md#build-the-native-frameworks)
- [Windows source build](../app/windows/README.md#build-the-application)

The Apple build creates both iOS and macOS slices in
`platform/apple/Framework/`. The Windows Flutter build compiles the adapter and
engine as part of the application CMake graph.

## Test and lint

From `clients/engine` on a fully provisioned development machine:

```bash
make all
make test
make lint
make clang-format
```

Live tests are intentionally separate because they open real network
connections. Routine unit tests must remain hermetic.

## Runtime privacy

The engine has no telemetry client, crash uploader, account API, or automatic
update connection. Logs are written only to local callbacks, files, or the
console selected by the host application. Do not log credentials, packet
payloads, or other secrets.

At runtime, the engine connects to the endpoint named by the operator's TOML.
It uses explicitly configured DNS upstreams, or preserves the operating-system
DNS path when none are configured. Before changing routes or DNS, it uses the
captured operating-system resolver to resolve endpoint hostnames and hostname-
based encrypted DNS upstreams. If that resolver is unavailable, affected paths
fail closed; there is no built-in public fallback. Traffic entering its local
TUN or SOCKS listener is forwarded to destinations requested by local
applications. Package registries and source hosts used during compilation are
not application runtime destinations.

## Additional references

- [Native development details](DEVELOPMENT.md)
- [Command-line configuration reference](trusttunnel/README.md)
- [Platform adapters](platform/README.md)
- [License](LICENSE)
