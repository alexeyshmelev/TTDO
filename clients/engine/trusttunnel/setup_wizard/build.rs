use std::process::Command;

fn git_stdout(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn emit_git_rerun_paths() {
    for name in ["HEAD", "packed-refs", "refs/tags"] {
        if let Some(path) = git_stdout(&["rev-parse", "--git-path", name]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(reference) = git_stdout(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(path) = git_stdout(&["rev-parse", "--git-path", &reference]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}

/// Resolve the version, mirroring cmake/version.cmake's source order:
///   1. TT_CLIENT_VERSION environment variable (CI sets it once per job);
///   2. git describe --tags --match 'client-v*' (client tags are namespaced);
///   3. 0.0.0-git fallback (never hard-fails the build).
fn resolve_version() -> String {
    if let Ok(v) = std::env::var("TT_CLIENT_VERSION") {
        let v = v.trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }

    if let Ok(out) = Command::new("git")
        .args(["describe", "--tags", "--match", "client-v*"])
        .output()
    {
        if out.status.success() {
            let described = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(version) = described.strip_prefix("client-v") {
                if !version.is_empty() {
                    return version.to_string();
                }
            }
        }
    }

    "0.0.0-git".to_string()
}

/// Leading numeric X.Y.Z, dropping any -prerelease / +build suffix.
fn core_version(full: &str) -> String {
    let head: String = full
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let parts: Vec<&str> = head.split('.').collect();
    if parts.len() >= 3 && parts[..3].iter().all(|p| !p.is_empty()) {
        parts[..3].join(".")
    } else {
        "0.0.0".to_string()
    }
}

fn main() {
    // Re-run when the override or the resolved git state changes.
    println!("cargo:rerun-if-env-changed=TT_CLIENT_VERSION");
    emit_git_rerun_paths();
    println!("cargo:rerun-if-changed=resources/setup_wizard.rc.in");

    let full = resolve_version();
    let core = core_version(&full);
    let commas = core.replace('.', ",");

    // Inject into the crate so `env!("TT_CLIENT_VERSION")` (src/version.rs) works.
    println!("cargo:rustc-env=TT_CLIENT_VERSION={full}");

    // `commas` is only consumed by the Windows resource below.
    let _ = &commas;

    #[cfg(windows)]
    {
        // Render the version resource from its template and compile it. The
        // generated .rc lives in OUT_DIR so the source tree stays clean.
        let template = std::fs::read_to_string("resources/setup_wizard.rc.in")
            .expect("read resources/setup_wizard.rc.in");
        let rendered = template
            .replace("@TT_CLIENT_VERSION_COMMAS@", &commas)
            .replace("@TT_CLIENT_VERSION_FULL@", &full);

        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
        let rc_path = std::path::Path::new(&out_dir).join("setup_wizard.rc");
        std::fs::write(&rc_path, rendered).expect("write generated setup_wizard.rc");

        let _ = windres::Build::new().compile(rc_path.to_str().unwrap());
    }
}
