# Apple framework adapter for TrustTunnelClient

Build from `clients/engine/platform/apple` after bootstrapping Conan dependencies:

```bash
./build_framework.sh
# For a debug build:
./build_framework.sh -debug

ls Framework
```

The script creates `Framework/TrustTunnelClient.xcframework` and
`Framework/VpnClientFramework.xcframework`. The podspec is a local development
pod for these generated frameworks; consume it through the application
Podfiles' `:path` references, not from a remote CocoaPods source.

The script uses an explicit, unprefixed `TT_CLIENT_VERSION` when supplied;
otherwise it resolves the nearest `client-v*` tag. With no client tag it uses
`0.0.0-git`. Generic endpoint `v*` tags are ignored.
