# Development Guide

This monorepo contains the Rust endpoint, native client engine, and Flutter
application. Build output belongs only in ignored directories.

## Server development

The repository pins Rust 1.95 in `rust-toolchain.toml`. On Ubuntu, install the
native prerequisites before invoking Cargo:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends \
    build-essential clang cmake git libclang-dev make pkg-config
rustup show active-toolchain
make init
```

Build and run the endpoint from the repository root:

```bash
cargo build --bins
make endpoint/setup ENDPOINT_HOSTNAME=vpn.example.invalid
make endpoint/run LOG_LEVEL=info
```

Run every mandatory server check before committing:

```bash
make audit-source
make lint-rust
make lint-md
make test
```

The tests are hermetic and must not require a public service. See
[the configuration reference](CONFIGURATION.md) for endpoint settings and
[the source-build guide](docs/SOURCE_BUILDS.md) for production packaging.

## Client development

Client prerequisites and commands vary by operating system:

- [native engine development](clients/engine/README.md)
- [shared Flutter application](clients/app/README.md)
- [iOS build](clients/app/ios/README.md)
- [macOS build](clients/app/macos/README.md)
- [Windows build](clients/app/windows/README.md)

On a fully provisioned native-engine workstation, run:

```bash
cd clients/engine
make all
make test
make lint
make clang-format
```

For Flutter-only changes, run from `clients/app`:

```bash
flutter config --no-analytics
dart --disable-analytics
flutter pub get
flutter analyze
flutter test
```

The unified Linux CI runs the full server suite, a source-built native engine
build with tests and linters, and Flutter formatting, analysis, and tests.
Apple and Windows adapters still require their native SDKs. Treat these as
manual release gates: on a clean checkout of the release commit, complete the
[iOS](clients/app/ios/README.md), [macOS](clients/app/macos/README.md), and
[Windows](clients/app/windows/README.md) release builds and their documented
tests. Record the commit, tool versions, commands, and results with the release;
do not publish a unified client release if any gate was skipped or failed.

Apple frameworks, Windows libraries, Flutter bundles, Wintun binaries, client
profiles, signing files, and generated certificates must remain untracked.

## Dependency and network policy

Cargo, Conan, CocoaPods, Flutter, and platform SDKs may fetch source, recipes,
or compiler artifacts during a build. Keep lock files and reviewed source
revisions synchronized. The installed endpoint and client must not gain a
telemetry service, remote log upload, automatic updater, or built-in public DNS
fallback.

Run `make audit-source` whenever dependency or build configuration changes. It
rejects compiled artifacts and known hidden runtime destinations.

## Benchmarking

The local Docker benchmark builds both endpoint and client from this checkout.
See [the benchmark guide](bench/README.md). Benchmark results remain local
unless the operator explicitly copies them elsewhere.
