#!/usr/bin/env python3

import pathlib
import re
import sys


RUNTIME_ROOTS = (
    "endpoint/src",
    "lib/src",
    "tools/setup_wizard",
    "clients/app/lib",
    "clients/app/swift_common",
    "clients/app/ios",
    "clients/app/macos",
    "clients/app/windows/runner",
    "clients/engine/common/src",
    "clients/engine/common/include",
    "clients/engine/core/src",
    "clients/engine/core/include",
    "clients/engine/net/src",
    "clients/engine/net/include",
    "clients/engine/tcpip/src",
    "clients/engine/tcpip/include",
    "clients/engine/trusttunnel/src",
    "clients/engine/trusttunnel/include",
    "clients/engine/platform/apple",
    "clients/engine/platform/windows",
)

SOURCE_SUFFIXES = {".c", ".cc", ".cpp", ".dart", ".h", ".hpp", ".m", ".mm", ".rs", ".swift"}
DENIED_LITERALS = {
    "1.1.1.1": "built-in public DNS fallback",
    "8.8.8.8": "built-in public DNS fallback",
    "8.8.4.4": "built-in public DNS fallback",
    "46.243.231.30": "built-in vendor DNS fallback",
    "46.243.231.31": "built-in vendor DNS fallback",
    "2a10:50c0::1:ff": "built-in vendor DNS fallback",
    "2a10:50c0::2:ff": "built-in vendor DNS fallback",
    "ipv4only.arpa": "unrelated DNS health-check destination",
}
DENIED_DEPENDENCY_PATTERNS = (
    re.compile(r"\bsentry\s*="),
    re.compile(r"\bfirebase_analytics\s*:"),
    re.compile(r"\bcrashlytics\b", re.IGNORECASE),
)
URL_LITERAL = re.compile(r'''["'](https?://[^"'\s]+)''')
ALLOWED_TEST_HOSTS = (".example.invalid", ".example.test")
SENSITIVE_LOG_PATTERNS = (
    (
        re.compile(
            r"\b(?:log_[a-z_]+|[a-z_]*log)\s*\([^;]{0,800}"
            r"\brequest\.str\(\)",
            re.DOTALL,
        ),
        "serialized HTTP request in diagnostic log",
    ),
    (
        re.compile(
            r"\b(?:log_[a-z_]+|[a-z_]*log)\s*\([^;]{0,800}"
            r"streamable_to_string\s*\(\s*config\s*\[\s*"
            r'"(?:endpoint|listener)"',
            re.DOTALL,
        ),
        "credential-bearing configuration table in diagnostic log",
    ),
    (
        re.compile(
            r"log_sess\s*\([^;]{0,400}std::string_view\s*"
            r"\{\s*at\s*,\s*length\s*\}",
            re.DOTALL,
        ),
        "raw HTTP/1 header fragment in diagnostic log",
    ),
    (
        re.compile(
            r"log_frsid\s*\([^;]{0,700}\bvalue=\{\}",
            re.DOTALL,
        ),
        "raw HTTP/2 header value in diagnostic log",
    ),
    (
        re.compile(
            r"log_id!\s*\([^;]{0,900}"
            r'"(?:Sending|Received)[^"]*\{:\?\}"'
            r"[^;]{0,300}\bresponse\b",
            re.DOTALL,
        ),
        "raw HTTP response in diagnostic log",
    ),
)
REQUEST_PARTS_LOG = re.compile(
    r"log_id!\s*\((?P<body>[^;]{0,1200}\.request\(\)\.request\(\)"
    r"[^;]{0,1200})\);",
    re.DOTALL,
)


def is_generated_or_test(path):
    lowered = {part.lower() for part in path.parts}
    return bool(lowered & {"generated", "pigeon", "test", "tests"})


def source_files(root):
    for relative_root in RUNTIME_ROOTS:
        directory = root / relative_root
        if not directory.exists():
            continue
        for path in directory.rglob("*"):
            if path.is_file() and path.suffix.lower() in SOURCE_SUFFIXES:
                relative = path.relative_to(root)
                if not is_generated_or_test(relative):
                    yield relative


def inspect_source(path, text):
    failures = []
    for literal, description in DENIED_LITERALS.items():
        if literal in text:
            failures.append(f"{description}: {literal}")
    for line_number, line in enumerate(text.splitlines(), 1):
        stripped = line.lstrip()
        if stripped.startswith(("//", "///", "/*", "*", "#")):
            continue
        for match in URL_LITERAL.finditer(line):
            url = match.group(1)
            host = url.split("/", 3)[2].split(":", 1)[0]
            if not host.endswith(ALLOWED_TEST_HOSTS):
                failures.append(f"hardcoded runtime URL on line {line_number}: {url}")
    for pattern, description in SENSITIVE_LOG_PATTERNS:
        for match in pattern.finditer(text):
            line_number = text.count("\n", 0, match.start()) + 1
            failures.append(f"{description} on line {line_number}")
    for match in REQUEST_PARTS_LOG.finditer(text):
        if "scrub_request" not in match.group("body"):
            line_number = text.count("\n", 0, match.start()) + 1
            failures.append(
                f"unscrubbed HTTP request in diagnostic log on line {line_number}"
            )
    return [(path, failure) for failure in failures]


def audit(root):
    failures = []
    for path in source_files(root):
        failures.extend(inspect_source(path, (root / path).read_text(errors="replace")))

    for manifest in root.rglob("Cargo.toml"):
        if ".codex" in manifest.parts or "target" in manifest.parts:
            continue
        text = manifest.read_text(errors="replace")
        for pattern in DENIED_DEPENDENCY_PATTERNS:
            if pattern.search(text):
                failures.append((manifest.relative_to(root), f"telemetry dependency: {pattern.pattern}"))
    pubspec = root / "clients/app/pubspec.yaml"
    if pubspec.exists():
        text = pubspec.read_text(errors="replace")
        for pattern in DENIED_DEPENDENCY_PATTERNS:
            if pattern.search(text):
                failures.append((pubspec.relative_to(root), f"telemetry dependency: {pattern.pattern}"))
    return failures


def main():
    root = pathlib.Path(__file__).resolve().parent.parent
    failures = audit(root)
    if failures:
        for path, reason in failures:
            print(f"{path}: {reason}", file=sys.stderr)
        return 1
    print("Runtime source contains no hidden public fallback or telemetry destination.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
