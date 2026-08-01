#!/usr/bin/env python3

import pathlib
import tempfile
import unittest

import sys


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ENGINE_ROOT))

import conan_source


class SelectedSourceFilesTest(unittest.TestCase):
    def test_keeps_engine_and_shared_deeplink_only(self):
        selected = conan_source.selected_source_files(
            [
                "README.md",
                "clients/engine/CMakeLists.txt",
                "clients/engine/core/source.cpp",
                "deeplink/Cargo.toml",
                "endpoint/src/main.rs",
                "rust-toolchain.toml",
            ]
        )

        self.assertEqual(
            selected,
            (
                pathlib.PurePosixPath("clients/engine/CMakeLists.txt"),
                pathlib.PurePosixPath("clients/engine/core/source.cpp"),
                pathlib.PurePosixPath("deeplink/Cargo.toml"),
                pathlib.PurePosixPath("rust-toolchain.toml"),
            ),
        )

    def test_rejects_parent_traversal(self):
        with self.assertRaisesRegex(RuntimeError, "Invalid Git source path"):
            conan_source.selected_source_files(["clients/engine/../../secret"])


class CopyMonorepoSourcesTest(unittest.TestCase):
    def test_preserves_monorepo_layout(self):
        with tempfile.TemporaryDirectory() as temporary_dir:
            root = pathlib.Path(temporary_dir)
            repo = root / "repo"
            destination = root / "export"
            engine_file = repo / "clients" / "engine" / "CMakeLists.txt"
            deeplink_file = repo / "deeplink" / "Cargo.toml"
            toolchain_file = repo / "rust-toolchain.toml"
            engine_file.parent.mkdir(parents=True)
            deeplink_file.parent.mkdir(parents=True)
            engine_file.write_text("project(client)\n", encoding="utf-8")
            deeplink_file.write_text("[package]\n", encoding="utf-8")
            toolchain_file.write_text(
                '[toolchain]\nchannel = "1.95.0"\n', encoding="utf-8"
            )

            conan_source.copy_monorepo_sources(
                repo,
                destination,
                [
                    "clients/engine/CMakeLists.txt",
                    "deeplink/Cargo.toml",
                    "rust-toolchain.toml",
                ],
            )

            self.assertEqual(
                (destination / "clients" / "engine" / "CMakeLists.txt").read_text(
                    encoding="utf-8"
                ),
                "project(client)\n",
            )
            self.assertEqual(
                (destination / "deeplink" / "Cargo.toml").read_text(
                    encoding="utf-8"
                ),
                "[package]\n",
            )
            self.assertEqual(
                (destination / "rust-toolchain.toml").read_text(encoding="utf-8"),
                '[toolchain]\nchannel = "1.95.0"\n',
            )

    def test_requires_root_toolchain_file(self):
        with tempfile.TemporaryDirectory() as temporary_dir:
            root = pathlib.Path(temporary_dir)
            repo = root / "repo"
            engine_file = repo / "clients" / "engine" / "CMakeLists.txt"
            deeplink_file = repo / "deeplink" / "Cargo.toml"
            engine_file.parent.mkdir(parents=True)
            deeplink_file.parent.mkdir(parents=True)
            engine_file.touch()
            deeplink_file.touch()

            with self.assertRaisesRegex(RuntimeError, "rust-toolchain.toml"):
                conan_source.copy_monorepo_sources(
                    repo,
                    root / "export",
                    ["clients/engine/CMakeLists.txt", "deeplink/Cargo.toml"],
                )


class EngineSourceRootTest(unittest.TestCase):
    @staticmethod
    def make_engine_root(path):
        path.mkdir(parents=True, exist_ok=True)
        (path / "core").mkdir()
        (path / "trusttunnel").mkdir()
        (path / "CMakeLists.txt").touch()
        (path / "conanfile.py").touch()

    def test_prefers_staged_monorepo(self):
        with tempfile.TemporaryDirectory() as temporary_dir:
            root = pathlib.Path(temporary_dir)
            engine = root / "clients" / "engine"
            self.make_engine_root(engine)

            self.assertEqual(conan_source.engine_source_root(root), engine)

    def test_accepts_direct_checkout(self):
        with tempfile.TemporaryDirectory() as temporary_dir:
            root = pathlib.Path(temporary_dir)
            self.make_engine_root(root)

            self.assertEqual(conan_source.engine_source_root(root), root)

    def test_rejects_unrelated_cmake_project(self):
        with tempfile.TemporaryDirectory() as temporary_dir:
            root = pathlib.Path(temporary_dir)
            (root / "CMakeLists.txt").touch()

            with self.assertRaisesRegex(RuntimeError, "engine sources are missing"):
                conan_source.engine_source_root(root)


if __name__ == "__main__":
    unittest.main()
