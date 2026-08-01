# TrustTunnel graphical client

This Flutter application provides one shared interface for iOS, macOS, and
Windows. It is not a hosted VPN service: the user supplies TOML exported by a
TrustTunnel server they control.

## Architecture

```text
Flutter widgets and TOML validation
              |
              v
generated Pigeon messages
        /                 \
       v                   v
Swift host             C++ Windows host
       |                   |
packet-tunnel         vpn_easy adapter
extension                  |
        \                  /
         v                v
       clients/engine native VPN core
                    |
                    v
        operator-configured TrustTunnel server
```

The app references only source within this repository:

- iOS and macOS Podfiles use the local pod at
  `../../engine/platform/apple`.
- Windows CMake uses `add_subdirectory` to compile and link the local adapter
  at `../../engine/platform/windows`.
- Dart packages are pinned by `pubspec.lock`.

No Apple framework, Windows library, or executable is checked in. Build the
native engine and app on the target operating system.

## Version metadata

The graphical host version comes from `pubspec.yaml`. Its embedded native
engine has an independent version resolved from `TT_CLIENT_VERSION`, a
`client-v*` tag, or the honest `0.0.0-git` fallback. Record both version
surfaces and the full source commit when packaging an application; endpoint
`v*` tags do not version either client component.

## Common workstation setup

Use the Flutter revision recorded in `.metadata` for reproducibility. From a
directory beside this repository:

```bash
git clone https://github.com/flutter/flutter.git flutter-sdk
git -C flutter-sdk checkout 6fba2447e95c451518584c35e25f5433f14d888c
export PATH="$(pwd)/flutter-sdk/bin:$PATH"
flutter config --no-analytics
dart --disable-analytics
flutter doctor -v
```

On Windows, perform the equivalent checkout in PowerShell and add
`flutter-sdk\bin` to the current process `Path`. Do not run `flutter upgrade`;
that changes the compiler and generated project files beyond the pinned
revision.

The first `flutter` invocation downloads Flutter engine and Dart SDK artifacts.
`flutter pub get` downloads the exact packages in `pubspec.lock`. These are
build-time downloads, not runtime services built into the application.

After platform prerequisites are installed, validate the shared Dart layer
from `clients/app`:

```bash
flutter pub get
flutter analyze
flutter test
```

If `pigeon/input.dart` changes, regenerate every native bridge from this
directory and review all generated changes:

```bash
dart run pigeon --input pigeon/input.dart
flutter analyze
flutter test
```

Continue with the target guide:

- [iOS](ios/README.md)
- [macOS](macos/README.md)
- [Windows](windows/README.md)

## Configuration and logs

Use the [client configuration procedure](../README.md#create-and-transfer-a-client-configuration)
to create the endpoint's flat TOML export. Paste it into the editor and select
**Connect**. The app deterministically adds the native client defaults,
`[endpoint]`, and `[listener.tun]` before validation and connection. It leaves
an already-complete native client TOML document unchanged. Placeholder
addresses and credentials are rejected before the native VPN starts.

The application does not contain a public VPN endpoint or a vendor DNS
fallback. It does not contain analytics or crash-reporting SDKs. VPN logs are
local diagnostics:

- iOS and macOS store them in the app-group container shared by the host and
  packet-tunnel extension. **View Local Logs** reads a temporary local snapshot,
  and **Clear Local Logs** removes the local records.
- Windows does not currently expose native log files through the Flutter
  interface.

Treat logs as sensitive because network diagnostics can reveal connection
metadata. Inspect them locally and clear them when no longer needed.

## Files that must stay untracked

Never commit any of the following:

- `.dart_tool/`, `.pub-cache/`, `.flutter-plugins-dependencies`, or `build/`
- `ios/Pods/`, `macos/Pods/`, `.symlinks/`, or Flutter ephemeral directories
- Apple archives, `.ipa`, `.app`, `.xcarchive`, `.framework`, or
  `.xcframework` output
- Windows `.exe`, `.dll`, `.lib`, `.pdb`, or generated bundle directories
- provisioning profiles, signing keys, certificates, or client TOML

Run `git status --short` from the repository root after every build.
