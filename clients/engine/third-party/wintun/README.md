# Wintun Integration Material

This directory contains only the Wintun 0.14.1 API header needed to compile
the Windows adapter. It does not contain a DLL, driver, import library, or
other compiled artifact.

The header identifies itself as `GPL-2.0 OR MIT`; this project uses it under
the MIT option. The complete terms are in [LICENSE-MIT.txt](LICENSE-MIT.txt).
`LICENSE.txt` contains the separate terms supplied for official prebuilt
Wintun DLLs so distributors can preserve them when they add a signed DLL to a
private application bundle.

See the [Windows source-build guide](../../../app/windows/README.md) for the
required version, checksum verification, architecture selection, and runtime
installation procedure. Do not commit a Wintun archive or DLL to this source
repository.
