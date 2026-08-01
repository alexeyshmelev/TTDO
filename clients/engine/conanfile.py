import os
import re
from os.path import join

from conan import ConanFile
from conan.tools.apple import is_apple_os
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout
from conan.tools.files import copy, patch, update_conandata
from conan.tools.scm import Git

from conan_source import copy_monorepo_sources, engine_source_root


class VpnLibsConan(ConanFile):
    name = "vpn-libs"
    license = "Apache-2.0"
    author = "TrustTunnel"
    url = "https://github.com/alexeyshmelev/TTDO"
    vcs_url = "https://github.com/alexeyshmelev/TTDO.git"
    description = "TrustTunnel client implementation"
    settings = "os", "compiler", "build_type", "arch"
    options = {
        "with_ghc": [True, False],
        "sanitize": [None, "ANY"],
        "capi_linux_exports": [True, False],
    }
    default_options = {
        "with_ghc": False,
        "sanitize": None,  # None means none
        "capi_linux_exports": False,
    }
    # A list of paths to patches. The paths must be relative to the conanfile directory.
    # They are applied in case of the version equals 777 and mostly intended to be used
    # for testing.
    patch_files = []
    exports = "conan_source.py"
    exports_sources = patch_files

    def requirements(self):
        self.requires("dns-libs/2.10.0@adguard/oss", transitive_headers=True)
        self.requires("native_libs_common/8.1.44@adguard/oss", transitive_headers=True)

        self.requires("brotli/1.1.0", transitive_headers=True)
        self.requires("cxxopts/3.1.1", transitive_headers=True)
        self.requires("http_parser/2.9.4", transitive_headers=True)
        self.requires("klib/2021-04-06@adguard/oss", transitive_headers=True)
        self.requires("ldns/2021-03-29@adguard/oss", transitive_headers=True)
        self.requires("libevent/2.1.11@adguard/oss", transitive_headers=True)
        self.requires("magic_enum/0.9.5", transitive_headers=True)
        self.requires("nghttp2/1.56.0@adguard/oss", transitive_headers=True)
        self.requires("nlohmann_json/3.12.0")
        self.requires("tomlplusplus/3.3.0")
        self.requires("zlib/1.3.1", transitive_headers=True)

        if "mips" not in str(self.settings.arch):
            self.requires("openssl/boring-2024-09-13@adguard/oss", transitive_headers=True, force=True)
        else:
            self.requires("openssl/3.1.5-quic1@adguard/oss", transitive_headers=True, force=True)

    def build_requirements(self):
        self.test_requires("gtest/1.14.0")
        self.test_requires("fmt/12.1.0")

    def configure(self):
        self.options["gtest"].build_gmock = False
        # Resolve conflict between pcre2 required from dns-libs and pcre2 required form native_libs_common
        self.options["pcre2"].build_pcre2grep = False
        self.options["dns-libs"].tcpip = False

    def export(self):
        # The exported sources carry no .git. Resolve the local version while the
        # recipe can still see the checkout, then pass it to CMake through
        # conandata.yml.
        git = Git(self)
        metadata = {}
        if self.version == "local":
            metadata["local_version"] = self._local_version(git)
        try:
            repo_git = Git(self, git.get_repo_root())
            metadata["source_commit"] = repo_git.get_commit()
        except Exception:
            pass
        if metadata:
            update_conandata(self, metadata)

    def export_sources(self):
        if self.version == "local":
            git = Git(self)
            repo_root = git.get_repo_root()
            repo_git = Git(self, repo_root)
            copy_monorepo_sources(
                repo_root,
                self.export_sources_folder,
                repo_git.included_files(),
            )

    @staticmethod
    def _git_described_version(git):
        # Quote the glob: Git.run executes through a shell, so an unquoted
        # "client-v*" would expand against files in the recipe directory.
        try:
            described = git.run('describe --tags --match "client-v*"').strip()
        except Exception:
            return ""
        prefix = "client-v"
        return described[len(prefix) :] if described.startswith(prefix) else ""

    @classmethod
    def _local_version(cls, git):
        explicit = os.environ.get("TT_CLIENT_VERSION", "").strip()
        return explicit or cls._git_described_version(git) or "0.0.0-git"

    def source(self):
        # Local exports preserve the monorepo paths needed by Rust path dependencies.
        try:
            engine_source_root(self.source_folder)
            return
        except RuntimeError:
            pass

        version = str(self.version)
        # A normalized describe version looks like "<tag>-<n>-g<rev>"; check
        # out the commit after "-g". Any other version is a client release tag.
        described = re.search(r"-g([0-9a-f]+)$", version)
        ref = (self.conan_data or {}).get("source_commit")
        if not ref:
            ref = described.group(1) if described else "client-v%s" % version
        git = Git(self)
        git.clone(url=self.vcs_url, target=".")
        git.checkout(ref)
        engine_source_root(self.source_folder)

    def generate(self):
        deps = CMakeDeps(self)
        deps.generate()
        tc = CMakeToolchain(self)
        # Don't write CMakeUserPresets.json into the source folder: builds are
        # driven by the presets in CMakePresets.json, and Conan would add one
        # include per build directory, so two build directories sharing a build
        # type yield duplicate `conan-<build type>` presets that make CMake
        # refuse to read the file.
        tc.user_presets_path = None
        # Drive cmake/version.cmake from the package version so the conan source
        # (no .git for git describe) bakes the right version into the build. For
        # "local" exports the describe version was stapled into conandata.yml at
        # export time; a release version is the package version itself.
        version = str(self.version)
        if version == "local":
            version = (self.conan_data or {}).get("local_version") or version
        if version and version != "local":
            tc.cache_variables["TT_CLIENT_VERSION"] = version
        if self.settings.os == "Linux" and self.options.capi_linux_exports:
            tc.cache_variables["VPNLIBS_CAPI_LINUX_EXPORTS"] = True
        if self.options.sanitize:
            tc.cache_variables["CMAKE_C_FLAGS"] += f" -fno-omit-frame-pointer -fsanitize={self.options.sanitize}"
            tc.cache_variables["CMAKE_CXX_FLAGS"] += f" -fno-omit-frame-pointer -fsanitize={self.options.sanitize}"
        tc.generate()

    def layout(self):
        cmake_layout(self)

    def build(self):
        cmake = CMake(self)
        source_root = engine_source_root(self.source_folder)
        build_script_folder = os.path.relpath(source_root, self.source_folder)
        cmake.configure(
            build_script_folder=None if build_script_folder == "." else build_script_folder
        )
        cmake.build(target="vpnlibs_common")
        cmake.build(target="vpnlibs_core")
        cmake.build(target="vpnlibs_net")
        cmake.build(target="vpnlibs_tcpip")
        cmake.build(target="vpnlibs_trusttunnel")

    def package(self):
        source_root = engine_source_root(self.source_folder)
        MODULES = [
            "common",
            "core",
            "net",
            "tcpip",
            "trusttunnel",
        ]

        for m in MODULES:
            copy(
                self,
                "*.h",
                src=join(source_root, "%s/include" % m),
                dst=join(self.package_folder, "include"),
                keep_path=True,
            )

        copy(
            self,
            "*.h",
            src=join(source_root, "third-party", "wintun", "include"),
            dst=join(self.package_folder, "include"),
            keep_path=True,
        )

        copy(self, "*.dll", src=self.build_folder, dst=join(self.package_folder, "bin"), keep_path=False)
        copy(self, "*.lib", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*.so", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*.dylib", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*.a", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)

    def package_info(self):
        self.cpp_info.name = "vpn-libs"
        self.cpp_info.libs = [
            "vpnlibs_trusttunnel",
            "vpnlibs_core",
            "vpnlibs_tcpip",
            "vpnlibs_net",
            "vpnlibs_common",
        ]
        if self.settings.os == "Windows":
            self.cpp_info.system_libs = ["ws2_32", "crypt32", "userenv", "version", "Fwpuclnt"]
        elif self.settings.os != 'Android':
            self.cpp_info.system_libs = ["resolv"]
        if is_apple_os(self):
            self.cpp_info.frameworks = ['Foundation', 'SystemConfiguration', 'Security']
