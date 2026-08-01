# TTDO Server Builds

This orphan branch stores server build artifacts separately from the
source-only `master` branch.

## server-build-20260801T125313Z

- Source: [`1cb00d7b3efe393d1242e4fa909e9ebb02dc40e1`](https://github.com/alexeyshmelev/TTDO/commit/1cb00d7b3efe393d1242e4fa909e9ebb02dc40e1)
- Build host: Ubuntu 20.04, `amd64`, glibc 2.31
- Rust: 1.95.0
- Endpoint version output: 1.0.41
- Archive SHA-256: `7a901e3c7d68eba28ebf32b337a296d304eefa43caecfb0f33246d95e98cf47e`

The executables were compiled from the locked source graph, stripped of debug
symbols for distribution, and then tested again. The folder contains the
individual executables and provenance files. The archive is the recommended
VPS transfer format.

Download and verify it on an `amd64` Ubuntu VPS:

```bash
base=https://raw.githubusercontent.com/alexeyshmelev/TTDO/server-builds/server-build-20260801T125313Z
curl --fail --location --remote-name \
    "$base/trusttunnel-server-ubuntu-20-04-amd64.tar.gz"
curl --fail --location --remote-name \
    "$base/trusttunnel-server-ubuntu-20-04-amd64.tar.gz.sha256"
sha256sum -c trusttunnel-server-ubuntu-20-04-amd64.tar.gz.sha256
tar --no-same-owner -xzf trusttunnel-server-ubuntu-20-04-amd64.tar.gz
cd trusttunnel
sha256sum -c BINARY_SHA256SUMS
```

The package is for x86-64 Linux. Check `TARGET_INFO`, `file`, and `ldd` before
replacing a working installation.
