from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout
from conan.tools.files import copy, get, patch
from os.path import join
import os


class NGHttp3Conan(ConanFile):
    name = "nghttp3"
    version = "1.0.0"
    settings = "os", "compiler", "build_type", "arch"
    options = {}
    default_options = {}
    exports_sources = ["CMakeLists.txt", "patches/*"]

    def source(self):
        get(
            self,
            url="https://github.com/ngtcp2/nghttp3/archive/7df838fcab8f4beef9c4aa7c109c18384b801b01.tar.gz",
            sha256="b3cdcd6d086c0a8ff229e54c500c1648dabb82079038c2add769758c54d8a063",
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
        tc.cache_variables["ENABLE_LIB_ONLY"] = "ON"
        tc.cache_variables["ENABLE_STATIC_LIB"] = "ON"
        tc.cache_variables["ENABLE_SHARED_LIB"] = "OFF"
        if tc.cache_variables.get("BUILD_TYPE") == "Debug":
            tc.cache_variables["DEBUGBUILD"] = "1"
        tc.generate()

    def layout(self):
        cmake_layout(self)

    def build(self):
        cmake = CMake(self)
        cmake.configure()
        cmake.build()

    def package(self):
        copy(self, "*.h", src=join(self.build_folder, "source_subfolder/lib/includes/nghttp3"), dst=join(self.package_folder, "include/nghttp3"), keep_path = True)
        copy(self, "*.h", src=join(self.source_folder, "source_subfolder/lib/includes/nghttp3"), dst=join(self.package_folder, "include/nghttp3"), keep_path = True)
        copy(self, "*.lib", self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*.a", self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)

    def package_info(self):
        self.cpp_info.libs = ["nghttp3"]
        self.cpp_info.defines.append("NGHTTP3_STATICLIB")
