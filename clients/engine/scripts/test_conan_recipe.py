#!/usr/bin/env python3

import ast
import json
import pathlib
import re
import unittest


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]


class ConanRecipeTest(unittest.TestCase):
    def test_static_libraries_follow_dependency_order(self):
        tree = ast.parse(
            (ENGINE_ROOT / "conanfile.py").read_text(encoding="utf-8")
        )
        package_info = next(
            node
            for node in ast.walk(tree)
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
            and node.name == "package_info"
        )
        libraries = next(
            ast.literal_eval(node.value)
            for node in ast.walk(package_info)
            if isinstance(node, ast.Assign)
            and any(
                isinstance(target, ast.Attribute)
                and target.attr == "libs"
                and isinstance(target.value, ast.Attribute)
                and target.value.attr == "cpp_info"
                for target in node.targets
            )
        )

        self.assertEqual(
            libraries,
            [
                "vpnlibs_trusttunnel",
                "vpnlibs_core",
                "vpnlibs_tcpip",
                "vpnlibs_net",
                "vpnlibs_common",
            ],
        )

    def test_cargo_build_runs_below_pinned_toolchain(self):
        cmake = (ENGINE_ROOT / "trusttunnel" / "CMakeLists.txt").read_text(
            encoding="utf-8"
        )
        setup_wizard = re.search(
            r"ExternalProject_Add\(setup_wizard(?P<body>.*?)\n\s*\)",
            cmake,
            re.DOTALL,
        )

        self.assertIsNotNone(setup_wizard)
        self.assertIn("BUILD_IN_SOURCE TRUE", setup_wizard.group("body"))

    def test_cmake_provider_applies_repository_lock(self):
        provider = (ENGINE_ROOT / "cmake" / "conan_provider.cmake").read_text(
            encoding="utf-8"
        )

        self.assertIn('../conan.lock")', provider)
        self.assertIn('"--lockfile=${_CONAN_LOCKFILE}"', provider)

    def test_conan_lock_covers_platform_specific_dependencies(self):
        lock = json.loads((ENGINE_ROOT / "conan.lock").read_text(encoding="utf-8"))
        references = lock["requires"]

        for dependency in (
            "detours/2021-04-14@adguard/oss#",
            "openssl/3.1.5-quic1@adguard/oss#",
            "openssl/boring-2024-09-13@adguard/oss#",
        ):
            self.assertTrue(
                any(reference.startswith(dependency) for reference in references),
                f"conan.lock does not cover {dependency}",
            )

    def test_conan_lock_does_not_inject_prebuilt_build_tools(self):
        lock = json.loads((ENGINE_ROOT / "conan.lock").read_text(encoding="utf-8"))

        self.assertEqual([], lock["build_requires"])


if __name__ == "__main__":
    unittest.main()
