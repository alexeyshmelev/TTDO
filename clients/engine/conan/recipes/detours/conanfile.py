from conan import ConanFile
from conan.tools.files import copy, get
from os.path import join

class DetoursConan(ConanFile):
    name = "detours"
    version = "2021-04-14"
    settings = "os", "compiler", "build_type", "arch"
    options = {"shared": [True, False], "fPIC": [True, False]}
    default_options = {"shared": False, "fPIC": True}

    def config_options(self):
        if self.settings.os == "Windows":
            del self.options.fPIC

    def source(self):
        get(
            self,
            url="https://github.com/microsoft/Detours/archive/fe7216c037c898b1f65330eda85e6146b6e3cb85.tar.gz",
            sha256="9300a24bde450c097fd9f46dd0435e4a99c415734c37389c066cfebf7a4df2ea",
            destination="source_subfolder",
            strip_root=True,
        )

    def build(self):
        self.run("cd source_subfolder\\src && set CC=cl && set CXX=cl && nmake")

    def package(self):
        copy(self, "*.h", src=join(self.source_folder, "source_subfolder/include"), dst=join(self.package_folder, "include"), keep_path = True)
        copy(self, "*detours.lib", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)
        copy(self, "*detours.pdb", src=self.build_folder, dst=join(self.package_folder, "lib"), keep_path=False)

    def package_info(self):
        self.cpp_info.libs = ["detours"]
