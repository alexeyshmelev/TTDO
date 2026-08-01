from conan import ConanFile
from conan.tools.files import copy, get
from os.path import join


class KlibConan(ConanFile):
    name = "klib"
    version = "2021-04-06"
    package_type = "header-library"
    settings = "os", "compiler", "build_type", "arch"
    options = {"shared": [True, False], "fPIC": [True, False]}
    default_options = {"shared": False, "fPIC": True}

    def config_options(self):
        if self.settings.os == "Windows":
            del self.options.fPIC

    def source(self):
        get(
            self,
            url="https://github.com/attractivechaos/klib/archive/e1b2a40de8e2a46c05cc5dac9c6e5e8d15ae722c.tar.gz",
            sha256="067f6f71219197cee6d063ee3e774dfd19bbfb4cd20b519bc94280cacde4dba5",
            destination="klib",
            strip_root=True,
        )

    def package(self):
        copy(self, "khash.h", src=join(self.source_folder, "klib"), dst=join(self.package_folder, "include"), keep_path = True)
        copy(self, "kvec.h", src=join(self.source_folder, "klib"), dst=join(self.package_folder, "include"), keep_path = True)

    def package_info(self):
        self.cpp_info.includedirs = ["include"]
        self.cpp_info.libdirs = []
        self.cpp_info.libs = []
        self.cpp_info.bindirs = []
        self.cpp_info.resdirs = []
