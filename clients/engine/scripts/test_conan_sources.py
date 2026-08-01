#!/usr/bin/env python3

import ast
import pathlib
import re
import unittest

import yaml


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]
RECIPES_ROOT = ENGINE_ROOT / "conan" / "recipes"
RECIPE_PATCHES_ROOT = ENGINE_ROOT / "conan" / "recipe_patches"
SHA256 = re.compile(r"[0-9a-f]{64}")
COMMIT_ARCHIVE = re.compile(r"/[0-9a-f]{40}\.tar\.gz$")
CONANDATA_SOURCE = ast.dump(
    ast.parse('self.conan_data["sources"][self.version]', mode="eval").body
)


class ConanSourceTest(unittest.TestCase):
    def test_recipe_helpers_do_not_fetch_mutable_or_unverified_sources(self):
        for script in sorted(RECIPES_ROOT.glob("**/*.sh")):
            with self.subTest(script=script):
                source = script.read_text(encoding="utf-8")
                if not re.search(r"\b(?:curl|wget)\b[^\n]*https?://", source):
                    continue
                self.assertNotRegex(source, r"/(?:main|master)/")
                self.assertIn("sha256sum -c", source)

    def test_recipe_downloads_have_sha256_checksums(self):
        for recipe in sorted(RECIPES_ROOT.glob("*/conanfile.py")):
            with self.subTest(recipe=recipe.parent.name):
                tree = ast.parse(recipe.read_text(encoding="utf-8"))
                downloads = [
                    node
                    for node in ast.walk(tree)
                    if isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Name)
                    and node.func.id in {"download", "get"}
                ]

                for download in downloads:
                    expanded = [
                        keyword.value
                        for keyword in download.keywords
                        if keyword.arg is None
                    ]
                    url = next(
                        (
                            keyword.value.value
                            for keyword in download.keywords
                            if keyword.arg == "url"
                            and isinstance(keyword.value, ast.Constant)
                            and isinstance(keyword.value.value, str)
                        ),
                        None,
                    )
                    if url is None and len(download.args) > 1:
                        argument = download.args[1]
                        if isinstance(argument, ast.Constant) and isinstance(
                            argument.value, str
                        ):
                            url = argument.value
                    checksum = next(
                        (
                            keyword.value.value
                            for keyword in download.keywords
                            if keyword.arg == "sha256"
                            and isinstance(keyword.value, ast.Constant)
                            and isinstance(keyword.value.value, str)
                        ),
                        None,
                    )
                    if expanded:
                        self.assertEqual(
                            [CONANDATA_SOURCE],
                            [ast.dump(value) for value in expanded],
                            f"{recipe} expands an unreviewed download mapping",
                        )
                    else:
                        self.assertTrue(
                            checksum is not None and SHA256.fullmatch(checksum),
                            f"{recipe} has a download without a literal SHA-256 checksum",
                        )
                        self.assertTrue(
                            url is not None and COMMIT_ARCHIVE.search(url),
                            f"{recipe} does not download an immutable commit archive",
                        )

    def test_conandata_downloads_have_sha256_checksums(self):
        for conandata in sorted(RECIPES_ROOT.glob("*/conandata.yml")):
            with self.subTest(recipe=conandata.parent.name):
                recipe = ast.parse(
                    (conandata.parent / "conanfile.py").read_text(encoding="utf-8")
                )
                version = next(
                    node.value.value
                    for node in ast.walk(recipe)
                    if isinstance(node, ast.Assign)
                    and any(
                        isinstance(target, ast.Name) and target.id == "version"
                        for target in node.targets
                    )
                    and isinstance(node.value, ast.Constant)
                    and isinstance(node.value.value, str)
                )
                data = yaml.safe_load(conandata.read_text(encoding="utf-8"))
                source = data["sources"][version]
                checksum = source.get("sha256", "")
                self.assertIsNotNone(
                    SHA256.fullmatch(checksum),
                    f"{conandata} source {version} has no SHA-256 checksum",
                )
                self.assertIsNotNone(
                    COMMIT_ARCHIVE.search(source.get("url", "")),
                    f"{conandata} source {version} is not an immutable commit archive",
                )

    def test_recipes_do_not_clone_remote_sources(self):
        for recipe in sorted(RECIPES_ROOT.glob("*/conanfile.py")):
            with self.subTest(recipe=recipe.parent.name):
                tree = ast.parse(recipe.read_text(encoding="utf-8"))
                remote_git_calls = []
                for node in ast.walk(tree):
                    if not isinstance(node, ast.Call):
                        continue
                    if isinstance(node.func, ast.Attribute):
                        name = node.func.attr
                    elif isinstance(node.func, ast.Name):
                        name = node.func.id
                    else:
                        continue
                    if name in {"clone", "fetch_commit"}:
                        remote_git_calls.append(name)
                self.assertEqual([], remote_git_calls)

    def test_external_recipe_patches_pin_exported_commits(self):
        for patch_path in sorted(RECIPE_PATCHES_ROOT.glob("*.patch")):
            with self.subTest(patch=patch_path.name):
                source = patch_path.read_text(encoding="utf-8")
                additions = "\n".join(
                    line[1:]
                    for line in source.splitlines()
                    if line.startswith("+") and not line.startswith("+++")
                )
                self.assertIn('get("source_commit")', additions)
                self.assertIn('"source_commit": Git(self).get_commit()', additions)
                self.assertIn("git.checkout(ref)", source)
                self.assertIn("git.get_commit() != ref", source)
                self.assertNotIn("fetch_commit", additions)

    def test_external_recipe_patches_reject_prepopulated_release_sources(self):
        expected_guard = """if self.version == "local":
            if not os.path.isfile(source_marker):
                raise RuntimeError("The local source folder is incomplete")
        elif os.listdir(self.source_folder):
            raise RuntimeError("The release source folder must be empty")
        else:"""

        for patch_path in sorted(RECIPE_PATCHES_ROOT.glob("*.patch")):
            with self.subTest(patch=patch_path.name):
                additions = "\n".join(
                    line[1:]
                    for line in patch_path.read_text(encoding="utf-8").splitlines()
                    if line.startswith("+") and not line.startswith("+++")
                )
                self.assertIn(expected_guard, additions)

    def test_external_provider_download_is_immutable_and_checksummed(self):
        patch = (RECIPE_PATCHES_ROOT / "dns_libs_source_commit.patch").read_text(
            encoding="utf-8"
        )
        additions = "\n".join(
            line[1:]
            for line in patch.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        )

        self.assertIn("download(", additions)
        self.assertNotIn("/latest/", additions)
        self.assertRegex(
            additions,
            r"NativeLibsCommon/[\s\S]*[0-9a-f]{40}/cmake/conan_provider\.cmake",
        )
        self.assertRegex(
            additions,
            r'CONAN_PROVIDER_SHA256 = "[0-9a-f]{64}"',
        )
        self.assertIn("sha256=CONAN_PROVIDER_SHA256", additions)
        for unverified_downloader in ("file(DOWNLOAD", "curl ", "wget "):
            self.assertNotIn(unverified_downloader, additions)

    def test_dns_package_build_does_not_bootstrap_a_second_conan_graph(self):
        patch = (RECIPE_PATCHES_ROOT / "dns_libs_source_commit.patch").read_text(
            encoding="utf-8"
        )
        additions = "\n".join(
            line[1:]
            for line in patch.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        )

        self.assertIn("replace_in_file(", additions)
        self.assertIn("if(NOT DNSLIBS_CONAN_PACKAGE_BUILD ", additions)
        self.assertIn(
            'tc.cache_variables["DNSLIBS_CONAN_PACKAGE_BUILD"] = True',
            additions,
        )

    def test_dns_package_preserves_custom_doq_port_after_local_resolution(self):
        patch = (RECIPE_PATCHES_ROOT / "dns_libs_source_commit.patch").read_text(
            encoding="utf-8"
        )
        additions = "\n".join(
            line[1:]
            for line in patch.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        )

        self.assertIn('join(self.source_folder, "upstream", "upstream_doq.cpp")', additions)
        self.assertIn(
            "m_server_addresses.emplace_back(m_options.resolved_server_ip, m_port);",
            additions,
        )

    def test_nlc_package_build_does_not_bootstrap_a_second_conan_graph(self):
        patch = (
            RECIPE_PATCHES_ROOT / "native_libs_common_source_commit.patch"
        ).read_text(encoding="utf-8")
        additions = "\n".join(
            line[1:]
            for line in patch.splitlines()
            if line.startswith("+") and not line.startswith("+++")
        )

        self.assertIn("replace_in_file(", additions)
        self.assertIn(
            "if(NOT NATIVE_LIBS_COMMON_CONAN_PACKAGE_BUILD ", additions
        )
        self.assertIn(
            'tc.cache_variables["NATIVE_LIBS_COMMON_CONAN_PACKAGE_BUILD"] = True',
            additions,
        )

    def test_bootstrap_applies_both_external_recipe_patches(self):
        tree = ast.parse(
            (ENGINE_ROOT / "scripts" / "bootstrap_conan_deps.py").read_text(
                encoding="utf-8"
            )
        )
        applied_patches = {
            call.args[1].id
            for call in ast.walk(tree)
            if isinstance(call, ast.Call)
            and isinstance(call.func, ast.Name)
            and call.func.id == "apply_recipe_patch"
            and len(call.args) > 1
            and isinstance(call.args[1], ast.Name)
        }

        self.assertEqual(
            applied_patches,
            {"DNS_LIBS_RECIPE_PATCH", "NLC_RECIPE_PATCH"},
        )


if __name__ == "__main__":
    unittest.main()
