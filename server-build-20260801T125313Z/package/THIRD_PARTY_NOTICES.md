# Third-Party Notices

TrustTunnel is distributed under the [Apache License 2.0](LICENSE). The native
client source preserves third-party notices in place. This summary does not
replace those license texts.

## Vendored source

| Component | Use | License and notice location |
| --- | --- | --- |
| lwIP | Client-side TCP/IP stack | BSD 3-Clause; [complete notice](clients/engine/third-party/lwip/lwip/COPYING) |
| pcap save-file header | Optional packet-capture format | Original BSD license with advertising acknowledgement; [complete notice](clients/engine/third-party/pcap_savefile/include/pcap_savefile.h) |
| Wintun API header | Windows TUN integration | GPL-2.0 OR MIT; this project uses the [MIT option](clients/engine/third-party/wintun/LICENSE-MIT.txt) stated in the [header](clients/engine/third-party/wintun/include/wintun.h) |
| NativeLibsCommon build material | Conan recipes and provider | Apache License 2.0; [complete notice](clients/engine/third-party/native-libs-common-LICENSE.md) |
| Chromium registry lookup source | Public-suffix lookup used by the client | BSD 3-Clause; [complete notice](clients/engine/conan/recipes/tldregistry/chromium/LICENSE) |

The Wintun runtime DLL is not stored in this repository. If an operator adds
the official signed DLL to a Windows application bundle, the separate
[prebuilt-binary license](clients/engine/third-party/wintun/LICENSE.txt) must
remain with that distribution.

The pcap-derived source requires advertising materials that mention its
features or use to include this acknowledgement:

> This product includes software developed by the Computer Systems
> Engineering Group at Lawrence Berkeley Laboratory.

## Package-managed dependencies

Rust, Dart, CocoaPods, and Conan dependencies are identified by their manifests
and lock files. Their source and license files are resolved by the respective
build tools and are not copied into this Git tree. Before distributing a
binary, collect the exact licenses from the locked build environment and ship
all notices required by those dependencies. The server packaging procedure
generates `THIRD_PARTY_LICENSES.html` from the locked Rust graph with the
pinned `cargo-about` configuration in `scripts/licenses/`; a new or ambiguous
license makes that step fail for manual review. The configuration explicitly
checks and includes both the Rust wrapper license and the bundled BoringSSL
OpenSSL/SSLeay, ISC, and MIT terms from `boring-sys`.
It also checks the AWS-LC Rust wrapper and native-library license texts used by
Rustls.

Relevant inventories include:

- `Cargo.toml`, `Cargo.lock`, and `clients/engine/trusttunnel/Cargo.lock`;
- `clients/app/pubspec.yaml` and `clients/app/pubspec.lock`;
- Apple Podfiles and `clients/engine/platform/apple/Podfile.lock`; application
  Podfile locks are generated locally and ignored;
- `clients/engine/conanfile.py` and the reviewed recipes below
  `clients/engine/conan/`.
