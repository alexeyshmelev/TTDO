#!/usr/bin/env python3

"""
This script intended to fill the local conan cache with the packages required
for building the project. Clean build scenario requires running this script
before running the cmake command. Besides that, it may be also required after
the dependencies updates.

Usage:
    bootstrap_conan_deps.py [nlc_url [dns_libs_url]]

`nlc_url` is the URL of AdGuard's NativeLibsCommon repository
(defaults to https://github.com/AdguardTeam/NativeLibsCommon.git).
`dns_libs_url` is the URL of AdGuard's DnsLibs repository
(defaults to https://github.com/AdguardTeam/DnsLibs.git).
"""

import os
import shutil
import stat
import subprocess
import sys

import yaml

work_dir = os.path.dirname(os.path.realpath(__file__))
project_dir = os.path.dirname(work_dir)
nlc_dir_name = "native-libs-common"
dns_libs_dir_name = "dns-libs"
recipes_dir = os.path.join(project_dir, "conan", "recipes")
recipe_patches_dir = os.path.join(project_dir, "conan", "recipe_patches")
NLC_RECIPE_PATCH = os.path.join(
    recipe_patches_dir, "native_libs_common_source_commit.patch"
)
DNS_LIBS_RECIPE_PATCH = os.path.join(
    recipe_patches_dir, "dns_libs_source_commit.patch"
)
SOURCE_BUILD_POLICY_HELPER = os.path.join(
    recipe_patches_dir, "source_build_policy.py"
)
PINNED_NLC_REVISIONS = {
    "8.1.44": "d94ed6d10c50c13f921bda724d4661c01b7d70b0",
}
PINNED_DNS_LIBS_REVISIONS = {
    "2.10.0": "036681e011cfe93bffa30b6f11a7b751dd2c0add",
}
UNSCOPED_LOCAL_RECIPES = {"ada"}


def on_rm_tree_error(func, path, _):
    """
    Workaround for Windows behavior, where `shutil.rmtree`
    fails with an access error (read only file).
    So, attempt to add write permission and try again.
    """
    if not os.access(path, os.W_OK):
        os.chmod(path, stat.S_IWUSR)
        func(path)
    else:
        raise


def remove_dir_if_exists(dir_path):
    """Remove a directory if it exists, handling read-only files on Windows."""
    if os.path.exists(dir_path):
        os.chdir(work_dir)
        shutil.rmtree(dir_path, onerror=on_rm_tree_error)


def pinned_revision(name, version, revisions):
    """Return the reviewed source commit for a dependency version."""
    try:
        return revisions[version]
    except KeyError as error:
        raise RuntimeError(
            f"No reviewed {name} source revision is pinned for version {version}"
        ) from error


def checkout_pinned(repo_dir, name, version, revisions):
    """Check out and verify a reviewed dependency source commit."""
    expected = pinned_revision(name, version, revisions)
    subprocess.run(["git", "-C", repo_dir, "checkout", expected], check=True)
    actual = subprocess.run(
        ["git", "-C", repo_dir, "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if actual != expected:
        raise RuntimeError(f"{name} checkout mismatch: expected {expected}, got {actual}")


def apply_recipe_patch(repo_dir, patch_path):
    """Apply a reviewed patch to an external Conan recipe."""
    subprocess.run(
        ["git", "-C", repo_dir, "apply", "--check", patch_path], check=True
    )
    subprocess.run(["git", "-C", repo_dir, "apply", patch_path], check=True)
    shutil.copy2(
        SOURCE_BUILD_POLICY_HELPER,
        os.path.join(repo_dir, os.path.basename(SOURCE_BUILD_POLICY_HELPER)),
    )


def verify_exported_source_commit(reference, expected):
    """Verify that an exported recipe records the reviewed source commit."""
    cache_path = subprocess.run(
        ["conan", "cache", "path", reference],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    with open(os.path.join(cache_path, "conandata.yml"), "r") as file:
        actual = (yaml.safe_load(file) or {}).get("source_commit")
    if actual != expected:
        raise RuntimeError(
            f"{reference} source mismatch: expected {expected}, got {actual}"
        )


def export_recipe(repo_dir, version):
    """Export a checked-out external recipe under its reviewed version."""
    subprocess.run(
        [
            "conan",
            "export",
            ".",
            "--user",
            "adguard",
            "--channel",
            "oss",
            "--version",
            version,
        ],
        check=True,
        cwd=repo_dir,
    )


def export_conan(repo_dir, version):
    """Check out, patch, and export NativeLibsCommon and its recipes."""
    checkout_pinned(repo_dir, "NativeLibsCommon", version, PINNED_NLC_REVISIONS)
    apply_recipe_patch(repo_dir, NLC_RECIPE_PATCH)
    export_recipe(repo_dir, version)
    export_local_recipes(os.path.join(repo_dir, "conan", "recipes"))
    expected = pinned_revision(
        "NativeLibsCommon", version, PINNED_NLC_REVISIONS
    )
    verify_exported_source_commit(
        f"native_libs_common/{version}@adguard/oss", expected
    )


def export_local_recipes(source_dir=recipes_dir):
    """Export the reviewed recipes bundled with this source tree."""
    for entry in sorted(os.scandir(source_dir), key=lambda item: item.name):
        if entry.is_dir() and os.path.isfile(os.path.join(entry.path, "conanfile.py")):
            command = ["conan", "export", entry.path]
            if entry.name not in UNSCOPED_LOCAL_RECIPES:
                command.extend(["--user", "adguard", "--channel", "oss"])
            subprocess.run(
                command,
                check=True,
            )


def main():
    nlc_url = (sys.argv[1] if len(sys.argv) > 1
               else "https://github.com/AdguardTeam/NativeLibsCommon.git")
    dns_libs_url = (sys.argv[2] if len(sys.argv) > 2
                    else "https://github.com/AdguardTeam/DnsLibs.git")
    nlc_versions = []
    dns_libs_version = None

    with open(os.path.join(project_dir, "conanfile.py"), "r") as file:
        for line in map(str.strip, file.readlines()):
            if line.startswith('self.requires("native_libs_common/') \
                    and ('@adguard/oss"' in line):
                nlc_versions.append(line.split('@')[0].split('/')[1])
            elif line.startswith('self.requires("dns-libs/') \
                    and ('@adguard/oss"' in line):
                dns_libs_version = line.split('@')[0].split('/')[1]

    if dns_libs_version is None:
        raise RuntimeError("The dns-libs dependency is missing from conanfile.py")

    dns_libs_dir = os.path.join(work_dir, dns_libs_dir_name)
    remove_dir_if_exists(dns_libs_dir)
    try:
        subprocess.run(["git", "clone", dns_libs_url, dns_libs_dir], check=True)
        checkout_pinned(
            dns_libs_dir,
            "DnsLibs",
            dns_libs_version,
            PINNED_DNS_LIBS_REVISIONS,
        )
        apply_recipe_patch(dns_libs_dir, DNS_LIBS_RECIPE_PATCH)
        os.chdir(dns_libs_dir)
        with open("conanfile.py", "r") as file:
            for line in map(str.strip, file.readlines()):
                if line.startswith('self.requires("native_libs_common/') \
                        and ('@adguard/oss"' in line):
                    nlc_versions.append(line.split('@')[0].split('/')[1])

        export_recipe(dns_libs_dir, dns_libs_version)
        verify_exported_source_commit(
            f"dns-libs/{dns_libs_version}@adguard/oss",
            pinned_revision(
                "DnsLibs", dns_libs_version, PINNED_DNS_LIBS_REVISIONS
            ),
        )
    finally:
        remove_dir_if_exists(dns_libs_dir)

    os.chdir(work_dir)
    nlc_dir = os.path.join(work_dir, nlc_dir_name)
    remove_dir_if_exists(nlc_dir)
    try:
        subprocess.run(["git", "clone", nlc_url, nlc_dir], check=True)

        seen = set()
        for version in nlc_versions:
            if version in seen:
                continue
            seen.add(version)
            export_conan(nlc_dir, version)
    finally:
        remove_dir_if_exists(nlc_dir)

    export_local_recipes()


if __name__ == "__main__":
    main()
