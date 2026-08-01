#!/bin/sh

# Export vpn-libs at an explicit or component-tag-derived client version.
#
# TT_CLIENT_VERSION takes priority. Otherwise, `client-v1.1.5-rc.2` becomes
# `1.1.5-rc.2`, with the usual Git suffix between client tags. A checkout with
# no client tag is exported honestly as `0.0.0-git`. To build uncommitted work,
# use `conan create . --version local`.

set -e

cd "$(dirname "$0")/.."

if [ -n "${TT_CLIENT_VERSION:-}" ]; then
    version=$TT_CLIENT_VERSION
else
    described=$(git describe --tags --match "client-v*" 2>/dev/null || true)
    version=${described#client-v}
    if [ -z "$version" ]; then
        version=0.0.0-git
    fi
fi
conan export . --user adguard --channel oss --version "$version"
