from conan import ConanFile
from conan.tools.cmake import CMake, CMakeDeps, CMakeToolchain, cmake_layout
from conan.tools.files import get


class LibsodiumConan(ConanFile):
    name = "libsodium"
    version = "1.0.18"
    settings = "os", "compiler", "build_type", "arch"
    options = {"shared": [True, False], "fPIC": [True, False]}
    default_options = {"shared": False, "fPIC": True}
    exports_sources = ["CMakeLists.txt", "sodiumConfig.cmake.in"]

    def config_options(self):
        if self.settings.os == "Windows":
            del self.options.fPIC

    def source(self):
        get(
            self,
            url="https://github.com/jedisct1/libsodium/archive/45b09a607d596e40adbff9ab812e47d85175c053.tar.gz",
            sha256="c380afa57bea112a0ff8ce12ae2cf849f82b8746ea35671dc55465b1928c8c52",
            destination="libsodium",
            strip_root=True,
        )

    def generate(self):
        deps = CMakeDeps(self)
        deps.generate()
        tc = CMakeToolchain(self)
        tc.cache_variables["BUILD_SHARED_LIBS"]="OFF"
        tc.generate()

    def layout(self):
        cmake_layout(self)

    def build(self):
        cmake = CMake(self)
        cmake.configure()
        cmake.build()
        cmake.install()

    def package_info(self):
        if self.settings.os == "Windows":
            self.cpp_info.libs = ["libsodium"]
        else:
            self.cpp_info.libs = ["sodium"]

        self.cpp_info.defines.append("SODIUM_STATIC=1")
