from pathlib import Path, PurePosixPath
import shutil


SOURCE_DIRECTORIES = (
    PurePosixPath("clients/engine"),
    PurePosixPath("deeplink"),
)
SOURCE_FILES = (PurePosixPath("rust-toolchain.toml"),)


def _is_relative_to(path, parent):
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def selected_source_files(included_files):
    """Return reviewed monorepo paths needed to build the client engine."""
    selected = set()
    for name in included_files:
        path = PurePosixPath(name.replace("\\", "/"))
        if path.is_absolute() or ".." in path.parts:
            raise RuntimeError(f"Invalid Git source path: {name}")
        if path in SOURCE_FILES or any(
            path == scope or _is_relative_to(path, scope)
            for scope in SOURCE_DIRECTORIES
        ):
            selected.add(path)
    return tuple(sorted(selected, key=str))


def copy_monorepo_sources(repo_root, destination, included_files):
    """Copy the engine and shared deep-link crate with monorepo-relative paths."""
    repo_root = Path(repo_root)
    destination = Path(destination)
    selected = selected_source_files(included_files)

    for required in SOURCE_DIRECTORIES:
        if not any(
            path == required or _is_relative_to(path, required) for path in selected
        ):
            raise RuntimeError(f"Required source tree is missing from Git: {required}")
    for required in SOURCE_FILES:
        if required not in selected:
            raise RuntimeError(f"Required source file is missing from Git: {required}")

    for path in selected:
        source = repo_root.joinpath(*path.parts)
        target = destination.joinpath(*path.parts)
        if not source.is_file() and not source.is_symlink():
            raise RuntimeError(f"Git listed a source file that does not exist: {path}")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target, follow_symlinks=False)


def engine_source_root(source_folder):
    """Locate the engine in a staged monorepo or a direct local checkout."""
    source_folder = Path(source_folder)
    monorepo_engine = source_folder / "clients" / "engine"
    if _is_engine_root(monorepo_engine):
        return monorepo_engine
    if _is_engine_root(source_folder):
        return source_folder
    raise RuntimeError("TrustTunnel client engine sources are missing")


def _is_engine_root(path):
    return (
        (path / "CMakeLists.txt").is_file()
        and (path / "conanfile.py").is_file()
        and (path / "core").is_dir()
        and (path / "trusttunnel").is_dir()
    )
