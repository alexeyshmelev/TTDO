# Build the iOS client from source

The iOS client supports iOS 14 and newer. It consists of a Flutter host app and
an embedded Network Extension whose packet-tunnel provider runs the native
TrustTunnel engine.

```text
Runner.app
|-- Flutter interface
|-- TrustTunnelClient.framework
`-- Network Extension.appex
    `-- AGPacketTunnelProvider -> native VPN engine
```

No XCFramework or application binary is stored in Git. Xcode and CMake produce
all Apple native code on the build Mac.

## Prepare a fresh Mac

You cannot build an iOS application on Ubuntu or Windows. Use macOS with:

- the latest stable Xcode and its command-line tools
- the iOS platform SDK and at least one simulator runtime
- Python 3.13 or newer
- CMake 3.24 or newer, Ninja 1.13 or newer, and the repository-pinned
  Conan 2.31.1
- Rustup; the repository-root
  [`rust-toolchain.toml`](../../../rust-toolchain.toml) pins Rust 1.95
- CocoaPods 1.16.2 or a compatible newer 1.x release
- Git and GNU Make
- the pinned Flutter checkout described in
  [the shared app guide](../README.md#common-workstation-setup)

Configure Xcode after installing it:

```bash
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -runFirstLaunch
sudo xcodebuild -license
xcodebuild -downloadPlatform iOS
```

Install the remaining command-line tools with your trusted package manager.
For example, with Homebrew:

```bash
brew install python@3.13 cmake ninja rustup cocoapods
export PATH="$(brew --prefix rustup)/bin:$(brew --prefix python@3.13)/libexec/bin:$PATH"
rustup toolchain install 1.95.0
```

Then clone this repository, check out the revision you intend to build, install
the pinned Flutter checkout, and verify the complete toolchain:

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

Resolve every error under the Flutter **iOS toolchain** section before
continuing. A simulator build is useful for checking the UI, but exercise the
VPN on a physical device with a valid Network Extension provisioning profile.

## Build the native frameworks

From `clients/engine`, bootstrap the pinned Conan recipes and build the Apple
adapters:

```bash
python3 -m venv env
. env/bin/activate
python -m pip install --requirement requirements.txt
conan profile detect --force
SKIP_VENV=1 make bootstrap_deps
cd platform/apple
bash ./build_framework.sh
```

The default Conan integration builds C and C++ dependencies from downloaded
source. The Apple script builds device, simulator, Intel macOS, and Apple
silicon macOS slices, then creates these ignored local artifacts:

```text
clients/engine/platform/apple/Framework/
|-- VpnClientFramework.xcframework
`-- TrustTunnelClient.xcframework
```

The script uses the nearest `client-v*` tag or falls back to `0.0.0-git`. Set
an explicit, unprefixed `TT_CLIENT_VERSION` only when intentionally labeling a
reviewed client release build.

Do not add that directory to Git. To discard the output and rebuild cleanly,
remove only `clients/engine/platform/apple/build/` and
`clients/engine/platform/apple/Framework/`, then rerun the script.

## Install Flutter and pod dependencies

From `clients/app`:

```bash
flutter pub get
cd ios
pod install
cd ..
open ios/Runner.xcworkspace
```

Always open `Runner.xcworkspace`, not `Runner.xcodeproj`. The Podfile references
`../../engine/platform/apple` on disk, so CocoaPods consumes the frameworks you
just built rather than downloading a TrustTunnel binary.

## Configure identifiers and signing

The checked-in identifiers are examples:

```text
host app:          org.trusttunnel.client
packet extension:  org.trusttunnel.client.PacketTunnel
shared app group:  group.org.trusttunnel.client
```

Replace them with identifiers owned by your Apple Developer team. Use one
consistent base, for example:

```text
host app:          com.example.trusttunnel
packet extension:  com.example.trusttunnel.PacketTunnel
shared app group:  group.com.example.trusttunnel
```

In Certificates, Identifiers & Profiles:

1. Register explicit App IDs for the host and packet-tunnel extension.
2. Enable the Network Extensions capability with packet-tunnel-provider access
   for the required identifiers.
3. Register the app group and assign both App IDs to it.
4. Regenerate provisioning profiles after changing capabilities.

In Xcode, select your team for the **Runner** and **Network Extension** targets,
set their bundle identifiers, and configure **Signing & Capabilities**. Both
targets must use the same App Group. The extension must have the **Network
Extensions: Packet Tunnel** capability and remain embedded in Runner's **Embed
App Extensions** build phase.

Also replace the corresponding literals in:

- `ios/Runner/Runner.entitlements`
- `ios/Network Extension/Network_Extension.entitlements`
- `swift_common/NativeVpnInterfaceImpl.swift`

The `bundleIdentifier` passed to `VpnManager` must equal the packet extension's
bundle identifier, and `appGroup` must equal both entitlement files. Search for
old values before building:

```bash
rg 'org\.trusttunnel\.client|group\.org\.trusttunnel\.client|TC3Q7MAJXF' \
    ios swift_common
```

No output should remain after customization. Apple controls capability and
provisioning availability for each developer membership; a normal app
signature without the Network Extension entitlement cannot start this VPN.

## Build and run

First validate compilation without producing a distributable archive:

```bash
flutter analyze
flutter test
flutter build ios --release --no-codesign
```

After signing is configured, connect an unlocked development device and list
its identifier:

```bash
flutter devices
flutter run --release -d DEVICE_ID
```

Replace `DEVICE_ID` with the value printed by Flutter. For an archive that can
be distributed under your Apple account:

```bash
flutter build ipa --release
```

Review the archive in Xcode Organizer. Distribution still requires the
appropriate Apple program membership, profiles, certificates, and App Store or
managed deployment process.

## Load the server configuration

Export TOML by following
[the shared configuration procedure](../../README.md#create-and-transfer-a-client-configuration).
Use AirDrop, an encrypted local file transfer, or another authenticated channel
between devices you control. Do not use GitHub as configuration storage.

Open the TOML locally, paste its complete contents into the app, and select
**Connect**. iOS prompts the user to approve the VPN configuration the first
time. Keep `skip_verification = false`; a hostname or certificate mismatch is a
server deployment error, not a reason to disable verification.

## Privacy and generated data

The finished app has no analytics, crash reporter, remote log service, or
update checker. It connects to the configured TrustTunnel endpoint and any DNS
upstream explicitly present in TOML. Traffic from other apps is sent through
that tunnel to destinations those apps request.

The host and extension write diagnostics into their shared app-group container.
**View Local Logs** reads a local snapshot, and **Clear Local Logs** removes the
records. Neither action sends data over the network.

Keep these generated or sensitive items out of Git:

- `clients/engine/platform/apple/build/` and `Framework/`
- `clients/app/build/`, `ios/Pods/`, `.symlinks/`, and Flutter ephemeral files
- `.xcarchive`, `.ipa`, provisioning profiles, certificates, and private keys
- exported client TOML and copied log files

## Official references

- [Flutter iOS setup](https://docs.flutter.dev/platform-integration/ios/setup)
- [Apple packet-tunnel provider requirements](https://developer.apple.com/documentation/networkextension/nepackettunnelprovider)
- [Apple capability configuration](https://developer.apple.com/help/account/identifiers/enable-app-capabilities/)
- [Apple App Groups](https://developer.apple.com/documentation/xcode/configuring-app-groups)
- [CocoaPods installation behavior](https://guides.cocoapods.org/using/pod-install-vs-update.html)
