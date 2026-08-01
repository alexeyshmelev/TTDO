from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout
from conan.tools.files import copy, get, patch
from os.path import join
import os


class Ngtcp2Conan(ConanFile):
    name = "ngtcp2"
    version = "0.19.1"
    settings = "os", "compiler", "build_type", "arch"
    options = {"shared": [True, False], "fPIC": [True, False]}
    default_options = {"shared": False, "fPIC": True}
    requires = ["openssl/boring-2024-09-13@adguard/oss"]
    exports_sources = ["CMakeLists.txt", "patches/popcnt_old_cpu_fix.patch"]

    def config_options(self):
        if self.settings.os == "Windows":
            del self.options.fPIC

    def source(self):
        get(
            self,
            url="https://github.com/ngtcp2/ngtcp2/archive/01194ac0b90b1d2c014881705392cb65c85620f6.tar.gz",
            sha256="a1862ab74a66cf523c5c061f81063b149cbffc1b3619196b5f1602ea989055d6",
            destination="source_subfolder",
            strip_root=True,
        )
        # Apply all patches from the `patches` directory
        patches_path = os.path.join("patches")
        patches = sorted([f for f in os.listdir(patches_path) if os.path.isfile(os.path.join(patches_path, f))])
        for patch_name in patches:
            patch(self, base_path="source_subfolder", patch_file=os.path.join(patches_path, patch_name))

    def generate(self):
        deps = CMakeDeps(self)
        deps.generate()
        tc = CMakeToolchain(self)
        tc.cache_variables["OPENSSL_ROOT_DIR"] = self.dependencies["openssl"].package_folder.replace("\\", "/")
        tc.cache_variables["ENABLE_SHARED_LIB"] = "OFF"
        tc.cache_variables["ENABLE_OPENSSL"] = "OFF"
        tc.cache_variables["ENABLE_BORINGSSL"] = "ON"
        tc.cache_variables["HAVE_SSL_IS_QUIC"] = "ON"
        tc.cache_variables["HAVE_SSL_SET_QUIC_EARLY_DATA_CONTEXT"] = "ON"
        tc.generate()

    def layout(self):
        cmake_layout(self)

    def build(self):
        cmake = CMake(self)
        cmake.configure()
        cmake.build()

    def package(self):
        copy(self, "*.h", src=join(self.build_folder, "source_subfolder/lib/includes"), dst=join(self.package_folder, "include"), keep_path = True)
        copy(self, "*.h", src=join(self.source_folder, "source_subfolder/lib/includes"), dst=join(self.package_folder, "include"), keep_path = True)
        copy(self, "*.h", src=join(self.source_folder, "source_subfolder/crypto/includes"), dst=join(self.package_folder, "include"), keep_path = True)
        copy(self, "*.dll", src=self.build_folder, dst=join(self.package_folder, "bin"), keep_path=False)
        copy(self, "*.lib", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*.so", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*.dylib", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*.a", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)

    def package_info(self):
        self.cpp_info.libs = ["ngtcp2", "ngtcp2_crypto_boringssl"]
        self.cpp_info.defines.append("NGTCP2_STATICLIB=1")
