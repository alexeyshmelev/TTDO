# Native platform adapters

This directory contains the source adapters used by the graphical client:

- `apple/` builds the Swift and Objective-C++ bridge as local XCFrameworks for
  iOS and macOS.
- `windows/` builds the C++ bridge directly in the Windows Flutter CMake graph.

The shared Flutter application is in [`../../app`](../../app/README.md). Follow
its [iOS](../../app/ios/README.md),
[macOS](../../app/macos/README.md), or
[Windows](../../app/windows/README.md) guide for an end-to-end source build.
