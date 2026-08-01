#!/usr/bin/env python3

import pathlib
import subprocess
import tempfile
import unittest
from unittest import mock

import audit_build_inputs
import audit_runtime_policy
import audit_source_tree


class SourceTreePolicyTest(unittest.TestCase):
    def test_rejects_compiled_magic(self):
        self.assertIn("ELF", audit_source_tree.classify(pathlib.Path("tool"), b"\x7fELF"))
        self.assertIn("Windows", audit_source_tree.classify(pathlib.Path("tool"), b"MZ"))
        self.assertIn("ZIP", audit_source_tree.classify(pathlib.Path("bundle"), b"PK\x03\x04"))

    def test_allows_source_and_media_assets(self):
        self.assertIsNone(audit_source_tree.classify(pathlib.Path("main.rs"), b"fn main() {}"))
        self.assertIsNone(audit_source_tree.classify(pathlib.Path("icon.png"), b"\x89PNG\r\n"))

    def test_rejects_compiled_suffix_even_when_contents_are_text(self):
        self.assertIsNotNone(audit_source_tree.classify(pathlib.Path("fake.dll"), b"source"))

    def test_rejects_package_suffixes_and_compressed_magic(self):
        for name in ("package.crate", "package.deb", "package.whl", "package.tar.zst"):
            with self.subTest(name=name):
                self.assertIsNotNone(
                    audit_source_tree.classify(pathlib.Path(name), b"text")
                )
        for header in (b"\x1f\x8b", b"\xfd7zXZ\x00", b"\x28\xb5\x2f\xfd"):
            with self.subTest(header=header):
                self.assertIsNotNone(
                    audit_source_tree.classify(pathlib.Path("payload"), header)
                )

    def test_rejects_generated_configuration_and_key_files(self):
        for name in (
            "credentials.toml",
            "credentials.txt",
            "deeplink.txt",
            "endpoint_config.toml",
            "nested/vpn.toml",
            "hosts.toml",
            "rules.toml",
            "client.toml",
            "trusttunnel_client.toml",
            ".env",
            "id_ed25519",
            "server.key",
            "identity.p12",
        ):
            with self.subTest(name=name):
                self.assertIsNotNone(
                    audit_source_tree.classify(pathlib.Path(name), b"placeholder")
                )

    def test_rejects_private_key_material_regardless_of_filename(self):
        data = b"-----BEGIN " + b"PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----\n"
        self.assertIsNotNone(
            audit_source_tree.classify(pathlib.Path("certificate.pem"), data)
        )

    def test_rejects_private_key_material_beyond_initial_read(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            candidate = root / "late-marker.pem"
            candidate.write_bytes(
                b"x" * 5000 + b"-----BEGIN " + b"PRIVATE KEY-----\nsecret\n"
            )
            with mock.patch.object(
                audit_source_tree, "index_entries", return_value=[]
            ), mock.patch.object(
                audit_source_tree,
                "untracked_files",
                return_value=[pathlib.Path("late-marker.pem")],
            ):
                self.assertEqual(
                    audit_source_tree.audit(root),
                    [(pathlib.Path("late-marker.pem"), "private-key material")],
                )

    def test_audits_staged_blob_when_worktree_content_diverges(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            subprocess.run(["git", "init", "--quiet"], cwd=root, check=True)
            candidate = root / "identity.pem"
            candidate.write_bytes(
                b"-----BEGIN " + b"PRIVATE KEY-----\nstaged secret\n"
            )
            subprocess.run(["git", "add", "identity.pem"], cwd=root, check=True)
            candidate.write_bytes(b"-----BEGIN CERTIFICATE-----\npublic\n")

            self.assertEqual(
                audit_source_tree.audit(root),
                [(pathlib.Path("identity.pem"), "private-key material")],
            )

    def test_audits_staged_blob_deleted_from_worktree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            subprocess.run(["git", "init", "--quiet"], cwd=root, check=True)
            candidate = root / "identity.pem"
            candidate.write_bytes(
                b"-----BEGIN " + b"PRIVATE KEY-----\nstaged secret\n"
            )
            subprocess.run(["git", "add", "identity.pem"], cwd=root, check=True)
            candidate.unlink()

            self.assertEqual(
                audit_source_tree.audit(root),
                [(pathlib.Path("identity.pem"), "private-key material")],
            )

    def test_allows_documented_configuration_templates_and_certificates(self):
        self.assertIsNone(
            audit_source_tree.classify(
                pathlib.Path("credentials.toml.example"), b"username = 'example'"
            )
        )
        self.assertIsNone(
            audit_source_tree.classify(
                pathlib.Path("certificate.pem"),
                b"-----BEGIN CERTIFICATE-----\npublic\n-----END CERTIFICATE-----\n",
            )
        )

    def test_repository_candidate_has_no_policy_violations(self):
        root = pathlib.Path(__file__).resolve().parent.parent
        self.assertEqual(audit_source_tree.audit(root), [])

    def test_docker_context_excludes_local_runtime_data(self):
        root = pathlib.Path(__file__).resolve().parent.parent
        patterns = {
            line.strip()
            for line in (root / ".dockerignore").read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        }
        for required in (
            "**/certs/",
            "**/vpn.toml",
            "**/credentials.toml",
            "**/credentials.txt",
            "**/deeplink.txt",
            "**/hosts.toml",
            "**/rules.toml",
            "**/trusttunnel_client.toml",
            "**/endpoint_config.toml",
            "**/client.toml",
            "**/.env",
            "**/id_ed25519",
            "**/id_rsa",
            "**/*.key",
            "**/*.p12",
            "**/*.pfx",
            "**/*.pem",
            "**/*.log",
            "**/*.pcap",
            "**/*.pcapng",
        ):
            self.assertIn(required, patterns)


class RuntimePolicyTest(unittest.TestCase):
    def test_rejects_public_dns_and_runtime_url(self):
        failures = audit_runtime_policy.inspect_source(
            pathlib.Path("client.cpp"),
            'auto dns = "8.8.8.8";\nauto api = "https://telemetry.invalid/v1";',
        )
        self.assertEqual(len(failures), 2)

    def test_allows_reserved_configuration_example(self):
        failures = audit_runtime_policy.inspect_source(
            pathlib.Path("config.dart"),
            'const endpoint = "https://vpn.example.invalid";',
        )
        self.assertEqual(failures, [])

    def test_rejects_sensitive_http_logging(self):
        failures = audit_runtime_policy.inspect_source(
            pathlib.Path("transport.cpp"),
            'log_upstream(this, dbg, "{}", request.str());\n'
            'log_sess(session, trace, "{}", std::string_view{at, length});',
        )
        self.assertEqual(len(failures), 2)

    def test_requires_request_scrubbing(self):
        unsafe = audit_runtime_policy.inspect_source(
            pathlib.Path("handler.rs"),
            'log_id!(trace, id, "request: {:?}", x.request().request());',
        )
        safe = audit_runtime_policy.inspect_source(
            pathlib.Path("handler.rs"),
            'log_id!(trace, id, "request: {:?}", '
            'scrub_request(x.request().request()));',
        )
        self.assertEqual(len(unsafe), 1)
        self.assertEqual(safe, [])


class BuildInputPolicyTest(unittest.TestCase):
    def test_accepts_pinned_base_and_verified_download(self):
        dockerfile = """
FROM example.invalid/tool@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
RUN curl -fsSL https://example.invalid/tool.tar.gz -o /tmp/tool.tar.gz && \\
    echo "hash  /tmp/tool.tar.gz" | sha256sum -c -
"""
        self.assertEqual(audit_build_inputs.inspect_dockerfile(dockerfile), [])

    def test_rejects_mutable_base_and_unverified_download(self):
        dockerfile = """
FROM example.invalid/tool:latest
RUN curl -fsSL https://example.invalid/tool.tar.gz -o /tmp/tool.tar.gz
"""
        self.assertEqual(len(audit_build_inputs.inspect_dockerfile(dockerfile)), 2)

    def test_rejects_unverified_remote_add(self):
        dockerfile = """
FROM scratch
ADD https://example.invalid/tool /tool
"""
        self.assertEqual(len(audit_build_inputs.inspect_dockerfile(dockerfile)), 1)


if __name__ == "__main__":
    unittest.main()
