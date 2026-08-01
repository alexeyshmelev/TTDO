#!/usr/bin/env python3

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ENGINE_ROOT))

import conanfile as conan_recipe


GIT = shutil.which("git")
CMAKE = shutil.which("cmake")
SHELL = shutil.which("sh")


def run(command, cwd, env=None):
    return subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=True,
        capture_output=True,
        text=True,
    )


def git(repo, *args):
    return run([GIT, *args], repo).stdout.strip()


def commit(repo, name, contents):
    marker = repo / name
    marker.write_text(contents, encoding="utf-8")
    git(repo, "add", name)
    git(repo, "commit", "--quiet", "-m", name)


def prepare_repo(root):
    repo = root / "repo"
    cmake_dir = repo / "clients" / "engine" / "cmake"
    scripts_dir = repo / "clients" / "engine" / "scripts"
    cmake_dir.mkdir(parents=True)
    scripts_dir.mkdir(parents=True)
    shutil.copy2(ENGINE_ROOT / "cmake" / "version.cmake", cmake_dir)
    shutil.copy2(ENGINE_ROOT / "scripts" / "export_conan.sh", scripts_dir)

    git(repo, "init", "--quiet")
    git(repo, "config", "user.email", "version-test@example.invalid")
    git(repo, "config", "user.name", "Version Test")
    git(repo, "add", ".")
    git(repo, "commit", "--quiet", "-m", "initial")
    return repo


def cmake_version(repo, cache_version=None, environment_version=None):
    harness = repo / "resolve-version.cmake"
    result = repo / "resolved-version.txt"
    harness.write_text(
        "\n".join(
            [
                'include("${CMAKE_CURRENT_LIST_DIR}/clients/engine/cmake/version.cmake")',
                'file(WRITE "${RESULT_FILE}"',
                '    "${TT_CLIENT_VERSION_FULL}|${TT_CLIENT_VERSION_CORE}|${TT_CLIENT_VERSION_COMMAS}")',
                "",
            ]
        ),
        encoding="utf-8",
    )

    command = [CMAKE]
    if cache_version is not None:
        command.append(f"-DTT_CLIENT_VERSION={cache_version}")
    command.extend(
        [
            f"-DRESULT_FILE={result.as_posix()}",
            "-P",
            str(harness),
        ]
    )
    env = os.environ.copy()
    env.pop("TT_CLIENT_VERSION", None)
    if environment_version is not None:
        env["TT_CLIENT_VERSION"] = environment_version
    run(command, repo, env=env)
    return tuple(result.read_text(encoding="utf-8").split("|"))


def install_fake_conan(root):
    bin_dir = root / "bin"
    bin_dir.mkdir()
    executable = bin_dir / "conan"
    executable.write_text(
        '#!/bin/sh\nprintf "%s\\n" "$@" > "$TT_CONAN_ARGUMENTS"\n',
        encoding="utf-8",
    )
    executable.chmod(0o755)
    return bin_dir


def exported_conan_version(repo, root, environment_version=None):
    capture = root / "conan-arguments.txt"
    bin_dir = install_fake_conan(root)
    env = os.environ.copy()
    env.pop("TT_CLIENT_VERSION", None)
    if environment_version is not None:
        env["TT_CLIENT_VERSION"] = environment_version
    env["TT_CONAN_ARGUMENTS"] = str(capture)
    env["PATH"] = str(bin_dir) + os.pathsep + env.get("PATH", "")
    run(
        [SHELL, str(repo / "clients" / "engine" / "scripts" / "export_conan.sh")],
        repo,
        env=env,
    )
    arguments = capture.read_text(encoding="utf-8").splitlines()
    return arguments[arguments.index("--version") + 1]


@unittest.skipUnless(GIT and CMAKE, "Git and CMake are required")
class CmakeClientVersionTest(unittest.TestCase):
    def test_nearer_server_tag_is_ignored(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = prepare_repo(pathlib.Path(directory))
            git(repo, "tag", "client-v1.1.5-rc.2")
            commit(repo, "server-release", "server\n")
            git(repo, "tag", "v9.9.9")
            commit(repo, "client-change", "client\n")

            full, core, commas = cmake_version(repo)

            self.assertRegex(full, r"^1\.1\.5-rc\.2-2-g[0-9a-f]+$")
            self.assertEqual("1.1.5", core)
            self.assertEqual("1,1,5", commas)

    def test_server_only_tag_falls_back(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = prepare_repo(pathlib.Path(directory))
            git(repo, "tag", "v9.9.9")

            self.assertEqual(
                ("0.0.0-git", "0.0.0", "0,0,0"), cmake_version(repo)
            )

    def test_checkout_without_git_falls_back(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            repo = root / "source"
            cmake_dir = repo / "clients" / "engine" / "cmake"
            cmake_dir.mkdir(parents=True)
            shutil.copy2(ENGINE_ROOT / "cmake" / "version.cmake", cmake_dir)

            self.assertEqual(
                ("0.0.0-git", "0.0.0", "0,0,0"), cmake_version(repo)
            )

    def test_environment_override_beats_tags(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = prepare_repo(pathlib.Path(directory))
            git(repo, "tag", "client-v1.1.5-rc.2")

            self.assertEqual(
                ("2.3.4-local", "2.3.4", "2,3,4"),
                cmake_version(repo, environment_version="2.3.4-local"),
            )

    def test_cache_override_beats_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = prepare_repo(pathlib.Path(directory))
            git(repo, "tag", "client-v1.1.5-rc.2")

            self.assertEqual(
                ("3.4.5-cache", "3.4.5", "3,4,5"),
                cmake_version(
                    repo,
                    cache_version="3.4.5-cache",
                    environment_version="8.8.8-environment",
                ),
            )


@unittest.skipUnless(GIT and SHELL, "Git and a POSIX shell are required")
class ConanExportClientVersionTest(unittest.TestCase):
    def test_component_tag_is_normalized(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            repo = prepare_repo(root)
            git(repo, "tag", "client-v1.1.5-rc.2")

            self.assertEqual("1.1.5-rc.2", exported_conan_version(repo, root))

    def test_server_only_tag_falls_back(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            repo = prepare_repo(root)
            git(repo, "tag", "v9.9.9")

            self.assertEqual("0.0.0-git", exported_conan_version(repo, root))

    def test_environment_override_beats_tag(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            repo = prepare_repo(root)
            git(repo, "tag", "client-v1.1.5-rc.2")

            self.assertEqual(
                "2.3.4-explicit",
                exported_conan_version(
                    repo, root, environment_version="2.3.4-explicit"
                ),
            )


class FakeGit:
    def __init__(self, result=None, error=None):
        self.result = result
        self.error = error
        self.commands = []

    def run(self, command):
        self.commands.append(command)
        if self.error is not None:
            raise self.error
        return self.result


class ConanRecipeClientVersionTest(unittest.TestCase):
    def test_component_tag_is_normalized(self):
        fake_git = FakeGit("client-v1.1.5-rc.2-3-gabc123\n")

        self.assertEqual(
            "1.1.5-rc.2-3-gabc123",
            conan_recipe.VpnLibsConan._git_described_version(fake_git),
        )
        self.assertEqual(
            ['describe --tags --match "client-v*"'], fake_git.commands
        )

    def test_non_client_tag_is_rejected(self):
        fake_git = FakeGit("v9.9.9\n")

        self.assertEqual(
            "", conan_recipe.VpnLibsConan._git_described_version(fake_git)
        )

    def test_local_environment_override_wins_without_git(self):
        fake_git = FakeGit(error=AssertionError("Git must not be called"))
        with mock.patch.dict(
            os.environ, {"TT_CLIENT_VERSION": " 2.3.4-explicit "}
        ):
            self.assertEqual(
                "2.3.4-explicit",
                conan_recipe.VpnLibsConan._local_version(fake_git),
            )
        self.assertEqual([], fake_git.commands)

    def test_local_version_without_override_or_client_tag_falls_back(self):
        fake_git = FakeGit(error=RuntimeError("no matching tag"))
        with mock.patch.dict(os.environ, {"TT_CLIENT_VERSION": ""}):
            self.assertEqual(
                "0.0.0-git",
                conan_recipe.VpnLibsConan._local_version(fake_git),
            )


class ClientVersionPolicyTest(unittest.TestCase):
    RESOLVERS = (
        "cmake/version.cmake",
        "conanfile.py",
        "scripts/export_conan.sh",
        "trusttunnel/setup_wizard/build.rs",
        "platform/apple/build_framework.sh",
        "platform/apple/TrustTunnelClient.podspec",
    )

    def test_all_git_resolvers_use_component_tags(self):
        for relative_path in self.RESOLVERS:
            with self.subTest(path=relative_path):
                contents = (ENGINE_ROOT / relative_path).read_text(encoding="utf-8")
                self.assertIn("client-v*", contents)
                self.assertNotIn("v*", contents.replace("client-v*", ""))

    def test_each_resolver_strips_the_component_prefix(self):
        expected = {
            "cmake/version.cmake": '"^client-v"',
            "conanfile.py": 'prefix = "client-v"',
            "scripts/export_conan.sh": "${described#client-v}",
            "trusttunnel/setup_wizard/build.rs": 'strip_prefix("client-v")',
            "platform/apple/build_framework.sh": "s/^client-v//",
            "platform/apple/TrustTunnelClient.podspec": "sub(/^client-v/, '')",
        }
        for relative_path, marker in expected.items():
            with self.subTest(path=relative_path):
                contents = (ENGINE_ROOT / relative_path).read_text(encoding="utf-8")
                self.assertIn(marker, contents)

    def test_conan_release_ref_uses_component_namespace(self):
        contents = (ENGINE_ROOT / "conanfile.py").read_text(encoding="utf-8")

        self.assertIn('"client-v%s" % version', contents)
        self.assertNotIn('else "v%s" % version', contents)

    def test_platform_consumers_use_resolved_values(self):
        trusttunnel_cmake = (ENGINE_ROOT / "trusttunnel" / "CMakeLists.txt").read_text(
            encoding="utf-8"
        )
        apple_script = (
            ENGINE_ROOT / "platform" / "apple" / "build_framework.sh"
        ).read_text(encoding="utf-8")
        client_resource = (
            ENGINE_ROOT / "trusttunnel" / "trusttunnel_client.rc.in"
        ).read_text(encoding="utf-8-sig")
        wizard_resource = (
            ENGINE_ROOT
            / "trusttunnel"
            / "setup_wizard"
            / "resources"
            / "setup_wizard.rc.in"
        ).read_text(encoding="utf-8-sig")

        self.assertIn("TT_CLIENT_VERSION=${TT_CLIENT_VERSION_FULL}", trusttunnel_cmake)
        self.assertIn('export TT_CLIENT_VERSION="${VER_FULL}"', apple_script)
        self.assertIn('MARKETING_VERSION="${VER_CORE}"', apple_script)
        for resource in (client_resource, wizard_resource):
            self.assertIn("@TT_CLIENT_VERSION_COMMAS@", resource)
            self.assertIn("@TT_CLIENT_VERSION_FULL@", resource)


if __name__ == "__main__":
    unittest.main()
