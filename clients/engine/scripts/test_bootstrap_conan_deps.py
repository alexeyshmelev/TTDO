#!/usr/bin/env python3

import importlib.util
import pathlib
import tempfile
import unittest
from unittest import mock

import yaml


MODULE_PATH = pathlib.Path(__file__).with_name("bootstrap_conan_deps.py")
SPEC = importlib.util.spec_from_file_location("bootstrap_conan_deps", MODULE_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

POLICY_MODULE_PATH = pathlib.Path(MODULE.SOURCE_BUILD_POLICY_HELPER)
POLICY_SPEC = importlib.util.spec_from_file_location(
    "source_build_policy", POLICY_MODULE_PATH
)
POLICY_MODULE = importlib.util.module_from_spec(POLICY_SPEC)
POLICY_SPEC.loader.exec_module(POLICY_MODULE)


class PinnedRevisionTest(unittest.TestCase):
    def test_external_recipes_force_transitive_source_builds(self):
        for patch_path in (
            MODULE.NLC_RECIPE_PATCH,
            MODULE.DNS_LIBS_RECIPE_PATCH,
        ):
            patch = pathlib.Path(patch_path).read_text(encoding="utf-8")
            additions = "\n".join(
                line[1:]
                for line in patch.splitlines()
                if line.startswith("+") and not line.startswith("+++")
            )
            source_hunk = patch.split("     def source(self):", 1)[1].split(
                "     def generate(self):", 1
            )[0]

            self.assertIn('exports = "source_build_policy.py"', additions)
            self.assertIn(
                "from source_build_policy import force_transitive_source_builds",
                additions,
            )
            self.assertIn(
                "+        force_transitive_source_builds(provider_path)", source_hunk
            )
            self.assertEqual(1, additions.count("force_transitive_source_builds(provider_path)"))
            self.assertNotIn('"--build=missing"', additions)

    def test_source_build_policy_is_idempotent_for_configured_sources(self):
        with tempfile.TemporaryDirectory() as directory:
            provider = pathlib.Path(directory) / "conan_provider.cmake"
            provider.write_text(
                "conan_install(--build=missing)\nconan_install(--build=missing)\n",
                encoding="utf-8",
            )

            POLICY_MODULE.force_transitive_source_builds(provider)
            configured = provider.read_text(encoding="utf-8")
            POLICY_MODULE.force_transitive_source_builds(provider)

            self.assertEqual(configured, provider.read_text(encoding="utf-8"))
            self.assertNotIn("--build=missing", configured)
            self.assertEqual(2, configured.count("--build=*"))

    def test_source_build_policy_rejects_unknown_provider(self):
        with tempfile.TemporaryDirectory() as directory:
            provider = pathlib.Path(directory) / "conan_provider.cmake"
            provider.write_text("conan_install()\n", encoding="utf-8")

            with self.assertRaisesRegex(RuntimeError, "no recognized build policy"):
                POLICY_MODULE.force_transitive_source_builds(provider)

    def test_returns_reviewed_revision(self):
        self.assertEqual(
            MODULE.pinned_revision(
                "DnsLibs", "2.10.0", MODULE.PINNED_DNS_LIBS_REVISIONS
            ),
            "036681e011cfe93bffa30b6f11a7b751dd2c0add",
        )

    def test_rejects_unreviewed_version(self):
        with self.assertRaisesRegex(RuntimeError, "No reviewed DnsLibs"):
            MODULE.pinned_revision(
                "DnsLibs", "next", MODULE.PINNED_DNS_LIBS_REVISIONS
            )

    def test_exports_bundled_recipes_in_stable_order(self):
        with tempfile.TemporaryDirectory() as directory:
            recipes = pathlib.Path(directory)
            (recipes / "z-recipe").mkdir()
            (recipes / "z-recipe" / "conanfile.py").touch()
            (recipes / "a-recipe").mkdir()
            (recipes / "a-recipe" / "conanfile.py").touch()
            (recipes / "ada").mkdir()
            (recipes / "ada" / "conanfile.py").touch()
            (recipes / "not-a-recipe").mkdir()

            with mock.patch.object(MODULE.subprocess, "run") as run:
                MODULE.export_local_recipes(directory)

            self.assertEqual(
                [call.args[0][2] for call in run.call_args_list],
                [
                    str(recipes / "a-recipe"),
                    str(recipes / "ada"),
                    str(recipes / "z-recipe"),
                ],
            )
            self.assertEqual(
                run.call_args_list[0].args[0][3:],
                ["--user", "adguard", "--channel", "oss"],
            )
            self.assertEqual(run.call_args_list[1].args[0][3:], [])
            self.assertEqual(
                run.call_args_list[2].args[0][3:],
                ["--user", "adguard", "--channel", "oss"],
            )
            self.assertTrue(all(call.kwargs["check"] for call in run.call_args_list))

    def test_exports_external_recipe_without_platform_shell(self):
        with mock.patch.object(MODULE.subprocess, "run") as run:
            MODULE.export_recipe("external-source", "1.2.3")

        run.assert_called_once_with(
            [
                "conan",
                "export",
                ".",
                "--user",
                "adguard",
                "--channel",
                "oss",
                "--version",
                "1.2.3",
            ],
            check=True,
            cwd="external-source",
        )

    def test_applies_external_recipe_patch_after_check(self):
        with mock.patch.object(MODULE.subprocess, "run") as run, mock.patch.object(
            MODULE.shutil, "copy2"
        ) as copy:
            MODULE.apply_recipe_patch("/tmp/source", "/tmp/source.patch")

        self.assertEqual(
            [call.args[0] for call in run.call_args_list],
            [
                [
                    "git",
                    "-C",
                    "/tmp/source",
                    "apply",
                    "--check",
                    "/tmp/source.patch",
                ],
                ["git", "-C", "/tmp/source", "apply", "/tmp/source.patch"],
            ],
        )
        self.assertTrue(all(call.kwargs["check"] for call in run.call_args_list))
        copy.assert_called_once_with(
            MODULE.SOURCE_BUILD_POLICY_HELPER,
            "/tmp/source/source_build_policy.py",
        )

    def test_verifies_exported_source_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            conandata = pathlib.Path(directory) / "conandata.yml"
            conandata.write_text(
                yaml.safe_dump({"source_commit": "a" * 40}), encoding="utf-8"
            )
            completed = mock.Mock(stdout=f"{directory}\n")
            with mock.patch.object(
                MODULE.subprocess, "run", return_value=completed
            ) as run:
                MODULE.verify_exported_source_commit("example/1.0@owner/channel", "a" * 40)

            run.assert_called_once_with(
                ["conan", "cache", "path", "example/1.0@owner/channel"],
                check=True,
                capture_output=True,
                text=True,
            )

    def test_rejects_exported_source_commit_mismatch(self):
        with tempfile.TemporaryDirectory() as directory:
            conandata = pathlib.Path(directory) / "conandata.yml"
            conandata.write_text(
                yaml.safe_dump({"source_commit": "b" * 40}), encoding="utf-8"
            )
            completed = mock.Mock(stdout=f"{directory}\n")
            with mock.patch.object(
                MODULE.subprocess, "run", return_value=completed
            ):
                with self.assertRaisesRegex(RuntimeError, "source mismatch"):
                    MODULE.verify_exported_source_commit(
                        "example/1.0@owner/channel", "a" * 40
                    )


if __name__ == "__main__":
    unittest.main()
