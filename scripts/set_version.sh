#!/bin/bash

# Set version in all project files.
#
# Usage: ./scripts/set_version.sh <version>
# Example: ./scripts/set_version.sh 1.2.0
#
# This script must be run from the project root directory.
# It updates version in:
#   - endpoint/Cargo.toml
#   - Cargo.lock

set -e

VERSION=$1

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 1.2.0"
    exit 1
fi

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Invalid version format. Expected X.Y.Z (e.g., 1.2.0)"
    exit 1
fi

echo "Setting version to $VERSION"

# endpoint/Cargo.toml
cargo_toml="endpoint/Cargo.toml"
OLD_VERSION=$(grep '^version = ' "$cargo_toml" | head -1 | sed -e 's/version = "\(.*\)"/\1/')
sed -i -e "s/^version = \"${OLD_VERSION}\"/version = \"${VERSION}\"/" "$cargo_toml"
echo "Updated ${cargo_toml}"

# Cargo.lock — regenerate to pick up the new version
cargo generate-lockfile
echo "Updated Cargo.lock"

echo "Version set to $VERSION in all project files."
