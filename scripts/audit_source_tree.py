#!/usr/bin/env python3

import os
import pathlib
import subprocess
import sys


DENIED_SUFFIXES = {
    ".a",
    ".7z",
    ".aar",
    ".apk",
    ".app",
    ".appx",
    ".bz2",
    ".cab",
    ".class",
    ".crate",
    ".deb",
    ".dll",
    ".dmg",
    ".dylib",
    ".exe",
    ".framework",
    ".gz",
    ".ipa",
    ".jar",
    ".lib",
    ".msi",
    ".msix",
    ".nupkg",
    ".o",
    ".obj",
    ".pdb",
    ".pkg",
    ".pyc",
    ".rar",
    ".rlib",
    ".rpm",
    ".so",
    ".wasm",
    ".whl",
    ".xcarchive",
    ".xcframework",
    ".xz",
    ".zip",
    ".zst",
}

DENIED_SECRET_FILENAMES = {
    ".env",
    "client.toml",
    "credentials.toml",
    "credentials.txt",
    "deeplink.txt",
    "endpoint_config.toml",
    "hosts.toml",
    "id_ed25519",
    "id_rsa",
    "rules.toml",
    "trusttunnel_client.toml",
    "vpn.toml",
}

DENIED_SECRET_SUFFIXES = {".key", ".p12", ".pfx"}

PRIVATE_KEY_MARKERS = (
    b"-----BEGIN " b"PRIVATE KEY-----",
    b"-----BEGIN " b"ENCRYPTED PRIVATE KEY-----",
    b"-----BEGIN " b"EC PRIVATE KEY-----",
    b"-----BEGIN " b"OPENSSH PRIVATE KEY-----",
    b"-----BEGIN " b"PGP PRIVATE KEY BLOCK-----",
    b"-----BEGIN " b"RSA PRIVATE KEY-----",
)

PRIVATE_KEY_SCAN_CHUNK_SIZE = 64 * 1024

MAGIC_HEADERS = {
    b"\x7fELF": "ELF executable or library",
    b"!<arch>\n": "static-library archive",
    b"\x00asm": "WebAssembly binary",
    b"PK\x03\x04": "ZIP or JAR archive",
    b"PK\x05\x06": "empty ZIP archive",
    b"\x1f\x8b": "gzip archive",
    b"\x28\xb5\x2f\xfd": "Zstandard archive",
    b"\x37\x7a\xbc\xaf\x27\x1c": "7-Zip archive",
    b"\x42\x5a\x68": "bzip2 archive",
    b"\xed\xab\xee\xdb": "RPM package",
    b"\xfd\x37\x7a\x58\x5a\x00": "XZ archive",
    b"Rar!\x1a\x07": "RAR archive",
    b"\xca\xfe\xba\xbe": "Java class or Mach-O universal binary",
    b"\xce\xfa\xed\xfe": "Mach-O binary",
    b"\xcf\xfa\xed\xfe": "Mach-O binary",
    b"\xfe\xed\xfa\xce": "Mach-O binary",
    b"\xfe\xed\xfa\xcf": "Mach-O binary",
}


def classify(path, data):
    """Return a reason when a path violates the source-tree policy."""
    lowered = path.as_posix().lower()
    filename = path.name.lower()
    if filename in DENIED_SECRET_FILENAMES:
        return "generated configuration or credential file"
    if any(filename.endswith(suffix) for suffix in DENIED_SECRET_SUFFIXES):
        return "private-key container or file"
    if any(marker in data for marker in PRIVATE_KEY_MARKERS):
        return "private-key material"
    if any(part.lower().endswith(tuple(DENIED_SUFFIXES)) for part in path.parts):
        return "compiled or packaged artifact suffix"
    if lowered.endswith(
        (".tar", ".tar.gz", ".tar.xz", ".tar.zst", ".tbz2", ".tgz", ".txz")
    ):
        return "source-tree archive"
    if data.startswith(b"MZ"):
        return "Windows PE executable or library"
    for header, description in MAGIC_HEADERS.items():
        if data.startswith(header):
            return description
    return None


def contains_private_key_marker(source, initial=b""):
    """Scan a binary stream for private-key markers across chunk boundaries."""
    overlap_size = max(map(len, PRIVATE_KEY_MARKERS)) - 1
    overlap = b""
    chunk = initial
    while chunk:
        data = overlap + chunk
        if any(marker in data for marker in PRIVATE_KEY_MARKERS):
            return True
        overlap = data[-overlap_size:]
        chunk = source.read(PRIVATE_KEY_SCAN_CHUNK_SIZE)
    return False


class LimitedReader:
    """Read at most one Git batch object's declared byte length."""

    def __init__(self, source, size):
        self.source = source
        self.remaining = size

    def read(self, size=-1):
        if self.remaining == 0:
            return b""
        if size < 0 or size > self.remaining:
            size = self.remaining
        if size == 0:
            return b""
        data = self.source.read(size)
        if not data:
            raise RuntimeError("Git ended an index blob before its declared size")
        self.remaining -= len(data)
        return data


def index_entries(root):
    """List stage-zero index entries as path, object ID, and mode tuples."""
    result = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--stage"],
        cwd=root,
        check=True,
        capture_output=True,
    )
    entries = []
    for item in result.stdout.split(b"\0"):
        if not item:
            continue
        metadata, encoded_path = item.split(b"\t", 1)
        mode, object_id, stage = metadata.split()
        if stage != b"0" or not object_id.strip(b"0"):
            continue
        entries.append(
            (
                pathlib.Path(os.fsdecode(encoded_path)),
                object_id.decode("ascii"),
                mode.decode("ascii"),
            )
        )
    return entries


def untracked_files(root):
    """List not-ignored files that are absent from the index."""
    result = subprocess.run(
        ["git", "ls-files", "-z", "--others", "--exclude-standard"],
        cwd=root,
        check=True,
        capture_output=True,
    )
    return [pathlib.Path(os.fsdecode(item)) for item in result.stdout.split(b"\0") if item]


def inspect_stream(path, source):
    """Return a source-policy violation found in one binary stream."""
    data = source.read(4096)
    reason = classify(path, data)
    if reason is None and contains_private_key_marker(source, data):
        reason = "private-key material"
    return reason


def audit_index(root):
    """Audit blobs exactly as they are staged in the Git index."""
    entries = [entry for entry in index_entries(root) if entry[2] != "160000"]
    if not entries:
        return []

    process = subprocess.Popen(
        ["git", "cat-file", "--batch"],
        cwd=root,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
    )
    if process.stdin is None or process.stdout is None:
        process.kill()
        process.wait()
        raise RuntimeError("Failed to open Git batch streams")

    failures = []
    try:
        for path, object_id, _ in entries:
            process.stdin.write(object_id.encode("ascii") + b"\n")
            process.stdin.flush()
            header = process.stdout.readline().rstrip(b"\n").split()
            if len(header) != 3 or header[1] != b"blob":
                raise RuntimeError(f"Git returned an invalid blob header for {path}")
            blob = LimitedReader(process.stdout, int(header[2]))
            reason = inspect_stream(path, blob)
            while blob.read(PRIVATE_KEY_SCAN_CHUNK_SIZE):
                pass
            if process.stdout.read(1) != b"\n":
                raise RuntimeError(f"Git returned an invalid blob terminator for {path}")
            if reason:
                failures.append((path, reason))
    except BaseException:
        process.kill()
        process.wait()
        process.stdin.close()
        process.stdout.close()
        raise

    process.stdin.close()
    returncode = process.wait()
    process.stdout.close()
    if returncode:
        raise subprocess.CalledProcessError(returncode, process.args)
    return failures


def audit_untracked(root):
    """Audit not-ignored working-tree files that are not staged."""
    failures = []
    for relative_path in untracked_files(root):
        path = root / relative_path
        if not path.is_file():
            continue
        with path.open("rb") as source:
            reason = inspect_stream(relative_path, source)
        if reason:
            failures.append((relative_path, reason))
    return failures


def audit(root):
    """Return violations from staged blobs and not-ignored untracked files."""
    return audit_index(root) + audit_untracked(root)


def main():
    root = pathlib.Path(__file__).resolve().parent.parent
    failures = audit(root)
    if failures:
        for path, reason in failures:
            print(f"{path}: {reason}", file=sys.stderr)
        return 1
    print("Source tree contains no prohibited artifacts or secrets.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
