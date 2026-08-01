# Client Source Provenance

The native client engine and initial graphical host application were imported
from the Apache-2.0 TrustTunnelClient project and then adapted for this
monorepo.

| Imported material | Upstream revision |
| --- | --- |
| Native engine, Apple adapter, Windows adapter, and original Flutter host | `886fefe74f722d65e04f34f767f67febe3b110ec` (`v1.1.5-rc.2`) |
| Vendored NativeLibsCommon Conan provider and recipes | `d94ed6d10c50c13f921bda724d4661c01b7d70b0` (`v8.1.44`) |
| DnsLibs recipe bootstrap revision | `036681e011cfe93bffa30b6f11a7b751dd2c0add` (`v2.10.0`) |

The checked-in DnsLibs recipe patch also changes its DoQ resolved-address path
to retain the port parsed from the configured URL instead of replacing it with
the default port. The Conan recipe revision in `clients/engine/conan.lock`
therefore identifies both the pinned upstream source and this reviewed local
fix.

The upstream `v1.1.5-rc.2` tag identifies the import baseline only. The
adaptations recorded under `Unreleased` mean this fork is not that exact
upstream release. Fork client releases use the separate `client-v*` tag
namespace; an untagged checkout reports `0.0.0-git` by default.

The imported Android adapter, release automation, prebuilt Gradle wrapper,
integration harness, and precompiled frameworks were intentionally excluded.
The Flutter host was refactored into `clients/app/`, restricted to iOS, macOS,
and Windows, and connected to `clients/engine/` through local build paths.

See [the client overview](README.md) and
[third-party notices](../THIRD_PARTY_NOTICES.md) for build and license details.
