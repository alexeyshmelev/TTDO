#!/usr/bin/env python3

import os
import pathlib
import stat
import subprocess
import tempfile
import textwrap
import unittest


REPOSITORY_ROOT = pathlib.Path(__file__).resolve().parents[1]
ENTRYPOINT = REPOSITORY_ROOT / "docker-entrypoint.sh"


class DockerEntrypointTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.directory.name)
        self.bin_directory = self.root / "bin"
        self.work_directory = self.root / "work"
        self.bin_directory.mkdir()
        self.work_directory.mkdir()
        self.setup_arguments = self.root / "setup-arguments"
        self.endpoint_arguments = self.root / "endpoint-arguments"

        self._write_executable(
            "setup_wizard",
            """
            #!/bin/sh
            printf '%s\n' "$@" > "$TT_TEST_SETUP_ARGUMENTS"
            printf 'generated\n' > credentials.toml
            printf 'generated\n' > vpn.toml
            printf 'generated\n' > hosts.toml
            printf 'generated\n' > rules.toml
            mkdir -p certs
            printf 'generated\n' > certs/cert.pem
            printf 'generated\n' > certs/key.pem
            """,
        )
        self._write_executable(
            "trusttunnel_endpoint",
            """
            #!/bin/sh
            printf '%s\n' "$@" > "$TT_TEST_ENDPOINT_ARGUMENTS"
            """,
        )

    def tearDown(self):
        self.directory.cleanup()

    def _write_executable(self, name, contents):
        path = self.bin_directory / name
        path.write_text(textwrap.dedent(contents).lstrip(), encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)

    def _environment(self):
        environment = os.environ.copy()
        environment.update(
            {
                "PATH": f"{self.bin_directory}:{environment['PATH']}",
                "TT_TEST_SETUP_ARGUMENTS": str(self.setup_arguments),
                "TT_TEST_ENDPOINT_ARGUMENTS": str(self.endpoint_arguments),
                "TT_HOSTNAME": "vpn.example.invalid",
            }
        )
        return environment

    def _run(self, environment):
        return subprocess.run(
            ["bash", str(ENTRYPOINT)],
            cwd=self.work_directory,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )

    def test_passes_credentials_path_without_secret_value(self):
        credentials = self.root / "credentials.input"
        credentials.write_text("alice:do-not-expose\n", encoding="utf-8")
        credentials.chmod(0o600)
        environment = self._environment()
        environment["TT_CREDENTIALS_FILE"] = str(credentials)

        result = self._run(environment)

        self.assertEqual(result.returncode, 0, result.stdout)
        arguments = self.setup_arguments.read_text(encoding="utf-8").splitlines()
        self.assertIn("--creds-file", arguments)
        self.assertIn(str(credentials), arguments)
        self.assertNotIn("do-not-expose", "\n".join(arguments))
        self.assertNotIn("do-not-expose", result.stdout)
        self.assertEqual(
            self.endpoint_arguments.read_text(encoding="utf-8").splitlines(),
            ["vpn.toml", "hosts.toml"],
        )

    def test_rejects_missing_credentials_file_without_invoking_wizard(self):
        environment = self._environment()
        environment["TT_CREDENTIALS_FILE"] = str(self.root / "missing")

        result = self._run(environment)

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Credentials file", result.stdout)
        self.assertFalse(self.setup_arguments.exists())

    def test_rejects_partial_configuration_without_overwriting_existing_files(self):
        credentials = self.work_directory / "credentials.toml"
        vpn = self.work_directory / "vpn.toml"
        credentials.write_text("credential-sentinel\n", encoding="utf-8")
        vpn.write_text("vpn-sentinel\n", encoding="utf-8")

        result = self._run(self._environment())

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Partial configuration detected", result.stdout)
        self.assertFalse(self.setup_arguments.exists())
        self.assertFalse(self.endpoint_arguments.exists())
        self.assertEqual(credentials.read_text(encoding="utf-8"), "credential-sentinel\n")
        self.assertEqual(vpn.read_text(encoding="utf-8"), "vpn-sentinel\n")

    def test_rejects_each_residual_wizard_output(self):
        for relative_path in ("rules.toml", "certs/cert.pem", "certs/key.pem"):
            with self.subTest(path=relative_path):
                sentinel = self.work_directory / relative_path
                sentinel.parent.mkdir(parents=True, exist_ok=True)
                sentinel.write_text("existing-state-sentinel\n", encoding="utf-8")

                result = self._run(self._environment())

                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Partial configuration detected", result.stdout)
                self.assertFalse(self.setup_arguments.exists())
                self.assertFalse(self.endpoint_arguments.exists())
                self.assertEqual(
                    sentinel.read_text(encoding="utf-8"),
                    "existing-state-sentinel\n",
                )
                sentinel.unlink()


if __name__ == "__main__":
    unittest.main()
