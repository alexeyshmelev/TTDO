# Native Client Development

The native engine is the shared implementation used by the command-line
client and the iOS, macOS, and Windows applications. Read the repository-level
[development guide](../../DEVELOPMENT.md) before changing it.

## Dependency flow

```text
common utilities
    |\
    | +--> net: sockets, TLS, HTTP, QUIC, DNS
    | +--> tcpip: lwIP packet handling
    |             |
    +-----------> core: VPN session and routing
                         |
                         +--> trusttunnel command-line client
                         +--> Apple adapter
                         `--> Windows adapter
```

The three Rust crates below `trusttunnel/` use the monorepo's `deeplink/`
crate through local path dependencies. Do not replace those paths with Git
dependencies.

## Configure dependencies

Install CMake 3.24 or newer, Ninja, Conan 2.31.1, Python 3, Rust 1.95, and a C++20
compiler. Repository linting requires clang-format 21 or newer.

Create a Conan profile and export the reviewed recipes:

```bash
python3 -m venv env
. env/bin/activate
python -m pip install --requirement requirements.txt
conan profile detect --force
python scripts/bootstrap_conan_deps.py
```

The bootstrap script clones DnsLibs and NativeLibsCommon source repositories,
checks out the full commit hashes recorded in the script, verifies each
checkout, and exports their recipes. CMake uses the vendored Conan provider and
passes `--build=*`, so C and C++ dependencies are compiled from source instead
of accepting prebuilt Conan packages.

Changing a dependency version requires reviewing its source, updating the
recipe, updating the matching full commit hash in
`scripts/bootstrap_conan_deps.py`, and updating its unit test.

## Build

On Linux or macOS:

```bash
make build_trusttunnel_client
make build_wizard
```

Select a different preset explicitly when needed:

```bash
make PRESET=clang-debug build_trusttunnel_client
make PRESET=clang-relwithdebinfo-sanitizer test
```

On Windows, use Developer PowerShell for Visual Studio 2022. These direct CMake
commands consume the checked-in MSVC preset and are the supported Windows path,
where GNU Make is not a prerequisite:

```powershell
python .\scripts\bootstrap_conan_deps.py
cmake --preset msvc-relwithdebinfo
cmake --build .\cmake-build-msvc-relwithdebinfo `
    --target trusttunnel_client setup_wizard tests
ctest --test-dir .\cmake-build-msvc-relwithdebinfo --output-on-failure
```

For graphical applications, follow the platform guides:

- [iOS](../app/ios/README.md)
- [macOS](../app/macos/README.md)
- [Windows](../app/windows/README.md)

## Version client releases

Use `client-v`-prefixed tags for native client releases. Root `v*` tags belong
to the endpoint and must never label client artifacts. The version after the
prefix must describe this fork's actual contents; the imported upstream tag in
`../PROVENANCE.md` is not a release tag for later monorepo changes.

For an intentional release build, either build at its reviewed `client-v*`
tag or set `TT_CLIENT_VERSION` to the unprefixed version. Without either, the
honest build version is `0.0.0-git`. Preserve the full source commit with every
artifact regardless of its human-readable version.

## Test and lint

Run all mandatory checks from `clients/engine`:

```bash
make all
make test
make lint
make clang-format
```

Routine tests must be hermetic. Tests labeled `live` are optional diagnostics
and must never be part of the default suite. New networking behavior needs a
local listener or mock rather than a public host.

The root source-policy checks cover the combined server and client tree:

```bash
cd ../..
make audit-source
```

## Generated files

Keep these outputs out of Git:

- `env/`, `cmake-build-*/`, `bin/`, and Conan caches;
- generated Rust `target/` directories;
- `platform/apple/Framework/`, Xcode archives, and derived data;
- Flutter and platform output below `../app/build/`;
- Windows executables, libraries, symbols, and Wintun DLLs.

The source tree intentionally contains the Wintun API header but not the
signed runtime DLL. See the Windows app guide for the separate download and
checksum procedure.

## Logging and destinations

Do not log credentials, authentication headers, packet payloads, or raw
configuration. Diagnostic callbacks and files remain local; do not add an
uploader or remote crash reporter.

The engine may connect only to an imported TrustTunnel endpoint, an explicitly
configured DNS upstream, the operating-system resolver selected by the device,
or destinations requested through the local TUN or SOCKS listener. Captured
system DNS is also used before tunnel changes to resolve configured endpoint
and encrypted-DNS hostnames. Do not add a vendor DNS fallback, account service,
update service, or unrelated health-check host.
