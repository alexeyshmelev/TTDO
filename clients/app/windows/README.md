# Build the Windows client from source

The Windows client is a Flutter desktop host that compiles and statically links
the C++ adapter and TrustTunnel engine from this repository.

```text
trusttunnel_client.exe
|-- Flutter Windows runner
|-- vpn_easy adapter
`-- TrustTunnel native engine
    `-- dynamically loads operator-supplied wintun.dll for TUN mode
```

The repository does not contain `.exe`, `.dll`, `.lib`, or driver files.

## Prepare a fresh Windows VM

Use a supported 64-bit Windows 10 or Windows 11 VM. Install:

- Git for Windows
- Python 3.13 or newer
- CMake 3.24 or newer and Ninja 1.13 or newer
- Conan 2.31.1, pinned by the native engine requirements
- Rustup; the repository-root `rust-toolchain.toml` pins Rust 1.95
- Visual Studio 2022, not Visual Studio Code, with **Desktop development with
  C++**, MSVC, CMake tools, and a Windows 10 or 11 SDK
- Strawberry Perl and NASM, which are needed by native cryptography builds
- the pinned Flutter checkout from
  [the shared app guide](../README.md#common-workstation-setup)

The [official Flutter Windows setup](https://docs.flutter.dev/platform-integration/windows/setup)
uses the Visual Studio **Desktop development with C++** workload. After installing
the tools, open **Developer PowerShell for VS 2022** and check them:

```powershell
git --version
python --version
cmake --version
ninja --version
cl
perl --version
nasm -v
rustup toolchain install 1.95.0
rustc --version
python -m pip install "conan==2.31.1"
conan profile detect --force
conan --version
```

Clone Flutter beside the repository and use the revision recorded in
`clients/app/.metadata`:

```powershell
git clone https://github.com/flutter/flutter.git flutter-sdk
git -C .\flutter-sdk checkout 6fba2447e95c451518584c35e25f5433f14d888c
$env:Path = "$(Resolve-Path .\flutter-sdk\bin);$env:Path"
flutter config --no-analytics --enable-windows-desktop
dart --disable-analytics
flutter doctor -v
```

Resolve every error in Flutter's **Windows toolchain** and **Visual Studio**
sections. Run the remaining commands in Developer PowerShell so `cl.exe` and
the Windows SDK environment are available.

## Bootstrap native dependencies

From `clients\engine`:

```powershell
conan profile detect --force
python .\scripts\bootstrap_conan_deps.py
```

The repository carries pinned Conan integration files and recipes. Its default
mode downloads dependency source and builds the C and C++ packages locally; it
does not fetch TrustTunnel engine binaries. This first build can take
considerable CPU time and disk space.

## Build the application

From `clients\app`:

```powershell
flutter pub get
flutter analyze
flutter test
flutter build windows --release
```

Flutter invokes `windows/CMakeLists.txt`. That file adds
`..\engine\platform\windows` to the same CMake graph and links `vpn_easy_a`, so
the engine is compiled from the adjacent source tree.

Find the generated bundle without assuming a Flutter-version-specific
architecture directory:

```powershell
$AppExe = Get-ChildItem .\build\windows -Recurse `
    -Filter trusttunnel_client.exe |
    Where-Object FullName -Match '\\Release\\' |
    Select-Object -First 1
if ($null -eq $AppExe) { throw "Release executable not found" }
$AppDir = $AppExe.Directory.FullName
$AppDir
```

Keep the complete generated directory together. The executable needs the
Flutter runtime DLL and `data` directory beside it.

## Supply and verify Wintun

System-wide TUN mode requires Wintun 0.14.1. Wintun's maintainers explicitly
state that their signed release DLL is the only supported distribution method;
a locally source-built driver is test-signed and is not a production
substitute. For that reason, Wintun is a documented runtime exception to the
source build. It is downloaded separately and never committed here.

Download the official archive, verify the SHA-256 value published on
[wintun.net](https://www.wintun.net/), and copy only the DLL matching the app
architecture:

```powershell
$Archive = Join-Path $env:TEMP 'wintun-0.14.1.zip'
$Extracted = Join-Path $env:TEMP 'trusttunnel-wintun-0.14.1'
$Expected = '07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51'
Invoke-WebRequest `
    'https://www.wintun.net/builds/wintun-0.14.1.zip' `
    -OutFile $Archive
$Actual = (Get-FileHash $Archive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($Actual -ne $Expected) { throw "Wintun checksum mismatch: $Actual" }
Remove-Item $Extracted -Recurse -Force -ErrorAction SilentlyContinue
Expand-Archive $Archive -DestinationPath $Extracted
$WintunDll = Join-Path $Extracted 'wintun\bin\amd64\wintun.dll'
if ((Get-AuthenticodeSignature $WintunDll).Status -ne 'Valid') {
    throw 'Wintun Authenticode signature is not valid'
}
Copy-Item $WintunDll (Join-Path $AppDir 'wintun.dll')
```

For an ARM64 application use `wintun\bin\arm64\wintun.dll`; for a 32-bit x86
application use `wintun\bin\x86\wintun.dll`. Never mix architectures. Keep the
archive's license with any package you distribute.

Delete the temporary archive and extracted folder when finished:

```powershell
Remove-Item $Archive -Force
Remove-Item $Extracted -Recurse -Force
```

Do not add `wintun.dll` to Git. A private release package may include the
verified signed DLL under its supplied license, but the source repository must
remain binary-free.

## Run and load configuration

Creating a Wintun adapter and changing system routes requires elevation. In
the same Developer PowerShell session where `$AppExe` was set, run the
following; `-Verb RunAs` requests elevation:

```powershell
Start-Process -FilePath $AppExe.FullName -Verb RunAs
```

Generate TOML with
[the shared configuration procedure](../../README.md#create-and-transfer-a-client-configuration),
transfer it directly to the Windows VM over an authenticated channel, and paste
it into the editor. Do not commit or publish the TOML because it contains VPN
credentials.

If startup reports that Wintun cannot be loaded, confirm that `wintun.dll` is
in `$AppDir`, that its architecture matches the application, and that the
signature remains valid. If the connection reaches the server but TLS fails,
correct the hostname, address, certificate, or system clock; do not set
`skip_verification = true` as a shortcut.

## Privacy and generated data

The finished app has no analytics, crash uploader, remote log collector,
account service, or update request. It connects to the endpoint and any DNS
upstream selected in TOML; tunneled application traffic reaches destinations
requested by those applications. Wintun is a local network driver and does not
add a telemetry destination.

The Windows Flutter bridge currently does not export native log files. Native
diagnostics remain in the local process output or debugger. Treat any captured
output as sensitive.

Never commit:

- `clients/app/.dart_tool/` or `clients/app/build/`
- `clients/engine/cmake-build-*`, its Python environment, or Conan cache output
- `.exe`, `.dll`, `.lib`, `.pdb`, installer, or symbol artifacts
- Wintun archives or DLLs
- exported client TOML, logs, signing certificates, or package keys

## Official references

- [Flutter Windows setup](https://docs.flutter.dev/platform-integration/windows/setup)
- [Flutter Windows build and bundle layout](https://docs.flutter.dev/platform-integration/windows/building)
- [Conan source-build modes](https://docs.conan.io/2/reference/commands/install.html#build-modes)
- [Wintun source, integration, and distribution policy](https://git.zx2c4.com/wintun/about/)
- [Wintun signed release and checksum](https://www.wintun.net/)
