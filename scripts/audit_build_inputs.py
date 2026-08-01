#!/usr/bin/env python3

import pathlib
import re
import sys


LOCAL_IMAGES = {"bench-common", "bench-ls", "scratch"}
DIGESTED_IMAGE = re.compile(r"@sha256:[0-9a-f]{64}$")


def docker_instructions(text):
    """Return logical Dockerfile instructions with continuations joined."""
    instructions = []
    current = ""
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not current and (not line or line.startswith("#")):
            continue
        current = f"{current} {line}".strip()
        if current.endswith("\\"):
            current = current[:-1].rstrip()
            continue
        instructions.append(current)
        current = ""
    if current:
        instructions.append(current)
    return instructions


def inspect_dockerfile(text):
    """Return reproducibility failures from a Dockerfile."""
    failures = []
    local_stages = set(LOCAL_IMAGES)
    for instruction in docker_instructions(text):
        command, _, body = instruction.partition(" ")
        command = command.upper()
        if command == "FROM":
            parts = body.split()
            image = parts[0] if parts else ""
            if image not in local_stages and not DIGESTED_IMAGE.search(image):
                failures.append(f"base image is not pinned by digest: {image}")
            if len(parts) >= 3 and parts[-2].upper() == "AS":
                local_stages.add(parts[-1])
        elif command == "ADD" and re.search(r"https?://", body):
            if not re.search(r"--checksum=sha256:[0-9a-f]{64}(?:\s|$)", body):
                failures.append("remote ADD is not pinned by SHA-256")
        elif command == "RUN" and re.search(r"\b(?:curl|wget)\b[^;&|]*https?://", body):
            if "sha256sum -c" not in body:
                failures.append("downloaded build input is not verified by SHA-256")
    return failures


def audit(root):
    """Return unpinned build inputs in repository Dockerfiles."""
    failures = []
    for path in root.rglob("Dockerfile"):
        if ".codex" in path.parts:
            continue
        for failure in inspect_dockerfile(path.read_text(errors="replace")):
            failures.append((path.relative_to(root), failure))
    return failures


def main():
    root = pathlib.Path(__file__).resolve().parent.parent
    failures = audit(root)
    if failures:
        for path, reason in failures:
            print(f"{path}: {reason}", file=sys.stderr)
        return 1
    print("Docker build inputs are pinned and checksum-verified.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
