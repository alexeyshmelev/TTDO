# Build the macOS client from source

The macOS client supports macOS 11 and newer. It combines a Flutter host,
the locally built TrustTunnel Apple frameworks, and a packet-tunnel app
extension.

```text
TrustTunnel client.app
|-- Flutter interface
|-- TrustTunnelClient.framework
`-- Network Extension.appex -> native VPN engine
```

No `.app`, framework, or XCFramework is checked in.

## Prepare a fresh Mac

Install:

- the latest stable Xcode and command-line tools
- the iOS platform SDK, because the shared framework script builds iOS and
  macOS slices together
- Python 3.13 or newer
- CMake 3.24 or newer, Ninja 1.13 or newer, and the repository-pinned
  Conan 2.31.1
- Rustup with the repository-pinned Rust 1.95 toolchain
- CocoaPods 1.16.2 or a compatible newer 1.x release
- Git, GNU Make, and the pinned Flutter checkout from
  [the shared app guide](../README.md#common-workstation-setup)

Initialize Xcode and the iOS SDK:

```bash
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -runFirstLaunch
sudo xcodebuild -license
xcodebuild -downloadPlatform iOS
```

For example, install the other tools with Homebrew:

```bash
brew install python@3.13 cmake ninja rustup cocoapods
export PATH="$(brew --prefix rustup)/bin:$(brew --prefix python@3.13)/libexec/bin:$PATH"
rustup toolchain install 1.95.0
```

Disable optional tool analytics and verify the environment:

```bash
flutter config --no-analytics
dart --disable-analytics
flutter doctor -v
python3 --version
cmake --version
ninja --version
rustc --version
pod --version
```

Resolve every error under Flutter's **macOS toolchain** section.

## Build the native frameworks

From `clients/engine`:

```bash
python3 -m venv env
. env/bin/activate
python -m pip install --requirement requirements.txt
conan profile detect --force
SKIP_VENV=1 make bootstrap_deps
cd platform/apple
bash ./build_framework.sh
```

Conan builds the pinned C and C++ dependencies from downloaded source by
default. The script creates
`platform/apple/Framework/VpnClientFramework.xcframework` and
`platform/apple/Framework/TrustTunnelClient.xcframework` locally. They contain
macOS and iOS slices and are ignored by Git.

The script uses the nearest `client-v*` tag or falls back to `0.0.0-git`. Set
an explicit, unprefixed `TT_CLIENT_VERSION` only when intentionally labeling a
reviewed client release build.

## Install Flutter and pod dependencies

From `clients/app`:

```bash
flutter pub get
cd macos
pod install
cd ..
open macos/Runner.xcworkspace
```

Open the workspace, not the Xcode project. The Podfile points at
`../../engine/platform/apple`; it does not download a prebuilt TrustTunnel
framework.

## Configure identifiers and signing

Replace the checked-in example identifiers with identifiers owned by your
Apple Developer team:

```text
host app:          org.trusttunnel.client
packet extension:  org.trusttunnel.client.PacketTunnel
shared app group:  group.org.trusttunnel.client
```

For example, use `com.example.trusttunnel`,
`com.example.trusttunnel.PacketTunnel`, and
`group.com.example.trusttunnel`. The iOS and macOS builds can use the same
multi-platform App IDs if your Apple account is configured that way.

In Certificates, Identifiers & Profiles, register the explicit host and
extension IDs, enable Network Extensions with packet-tunnel-provider access,
create the App Group, assign both IDs to it, and regenerate provisioning
profiles. In Xcode:

1. Select your team on **Runner** and **Network Extension**.
2. Set the host and extension bundle identifiers.
3. Give both targets the same **App Groups** capability.
4. Give the extension **Network Extensions: Packet Tunnel** and ensure it is
   embedded in the host.

Update the same values in:

- `macos/Runner/Configs/AppInfo.xcconfig`
- `macos/Runner/DebugProfile.entitlements`
- `macos/Runner/Release.entitlements`
- `macos/Network Extension/Network_Extension.entitlements`
- `swift_common/NativeVpnInterfaceImpl.swift`

The shared Swift `bundleIdentifier` must equal the packet extension ID, and its
`appGroup` must match every entitlement. Verify that no upstream team or
identifier remains:

```bash
rg 'org\.trusttunnel\.client|group\.org\.trusttunnel\.client|TC3Q7MAJXF' \
    macos swift_common
```

## Build and run

From `clients/app`:

```bash
flutter analyze
flutter test
flutter build macos --release
```

The build produces the application under
`build/macos/Build/Products/Release/`. For iterative development, select the
signed Runner scheme in Xcode or run:

```bash
flutter run -d macos
```

macOS asks the user to approve the VPN configuration and may require approval
in System Settings. Run end-to-end tests from the same user account that owns
the app-group container.

The current packet tunnel is packaged as an app extension. Apple documents
macOS packet-tunnel app-extension distribution as Mac App Store only. Local
development signing is suitable for development and testing; distributing
outside the Mac App Store with Developer ID requires a system-extension
packaging design and corresponding profiles. Do not publish the current archive
as a direct-download production VPN without completing that migration.

## Load the server configuration

Follow
[the shared configuration procedure](../../README.md#create-and-transfer-a-client-configuration)
to generate TOML. Transfer it directly between machines you control, paste it
into the app, and select **Connect**. Never put the TOML in GitHub: it contains
the VPN username and password.

Use a hostname covered by the server certificate and keep
`skip_verification = false`. Configure DNS explicitly if you need a particular
resolver.

## Privacy and generated data

The built app has no telemetry, crash-reporting upload, remote logging, vendor
account, or update request. Runtime control connections go only to the endpoint
and DNS upstream selected in TOML. TUN traffic reaches destinations requested
by local applications through the endpoint.

Logs stay in the Apple app-group container. Viewing logs creates a local
snapshot; clearing logs removes local files. Neither operation uploads data.

Never commit:

- `clients/engine/platform/apple/build/` or `Framework/`
- `clients/app/build/`, `macos/Pods/`, `.symlinks/`, or Flutter ephemeral files
- `.app`, `.xcarchive`, `.pkg`, `.dmg`, frameworks, or symbol bundles
- signing certificates, private keys, provisioning profiles, client TOML, or
  exported logs

## Official references

- [Flutter macOS setup](https://docs.flutter.dev/platform-integration/macos)
- [Flutter macOS builds and entitlements](https://docs.flutter.dev/platform-integration/macos/building)
- [Apple Network Extension deployment](https://developer.apple.com/documentation/technotes/tn3134-network-extension-provider-deployment)
- [Apple capability configuration](https://developer.apple.com/help/account/identifiers/enable-app-capabilities/)
- [Apple App Groups](https://developer.apple.com/documentation/xcode/configuring-app-groups)
