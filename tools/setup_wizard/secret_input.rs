use std::fs;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

const MAX_SECRET_FILE_SIZE: u64 = 1024 * 1024;

pub fn read_credentials(path: &Path) -> Result<(String, String), String> {
    let contents = read_protected_file(path, "credentials")?;
    parse_credentials(&contents)
}

fn parse_credentials(contents: &str) -> Result<(String, String), String> {
    let value = single_line(contents, "credentials")?;
    let (username, password) = value.split_once(':').ok_or_else(|| {
        "Credentials file must contain one line formatted as <username>:<password>".to_string()
    })?;

    if username.is_empty() {
        return Err("Credentials file username cannot be empty".to_string());
    }
    if password.is_empty() {
        return Err("Credentials file password cannot be empty".to_string());
    }

    Ok((username.to_string(), password.to_string()))
}

fn single_line<'a>(contents: &'a str, description: &str) -> Result<&'a str, String> {
    let value = contents
        .strip_suffix("\r\n")
        .or_else(|| contents.strip_suffix(['\r', '\n']))
        .unwrap_or(contents);
    if value.is_empty() {
        return Err(format!("{description} file cannot be empty"));
    }
    if value.contains(['\r', '\n']) {
        return Err(format!("{description} file must contain exactly one line"));
    }
    if value.contains('\0') {
        return Err(format!("{description} file cannot contain a NUL byte"));
    }
    Ok(value)
}

fn read_protected_file(path: &Path, description: &str) -> Result<String, String> {
    let initial_metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Cannot inspect {description} file '{}': {error}",
            path.display()
        )
    })?;
    if initial_metadata.file_type().is_symlink() {
        return Err(format!(
            "{description} file '{}' must not be a symbolic link",
            path.display()
        ));
    }
    if !initial_metadata.is_file() {
        return Err(format!(
            "{description} file '{}' must be a regular file",
            path.display()
        ));
    }

    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK);

    let mut file = options.open(path).map_err(|error| {
        format!(
            "Cannot open {description} file '{}': {error}",
            path.display()
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        format!(
            "Cannot inspect opened {description} file '{}': {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "{description} file '{}' must be a regular file",
            path.display()
        ));
    }
    if metadata.len() > MAX_SECRET_FILE_SIZE {
        return Err(format!(
            "{description} file '{}' exceeds the 1 MiB size limit",
            path.display()
        ));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "{description} file '{}' grants access to group or other users; run chmod 600 '{}'",
            path.display(),
            path.display()
        ));
    }

    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|error| {
        format!(
            "Cannot read {description} file '{}' as UTF-8 text: {error}",
            path.display()
        )
    })?;
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    #[cfg(unix)]
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NONCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            loop {
                let path = std::env::temp_dir().join(format!(
                    "trusttunnel-endpoint-secret-input-test-{}-{}",
                    std::process::id(),
                    NONCE.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("failed to create test directory: {error}"),
                }
            }
        }

        fn write_secret(&self, name: &str, contents: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, contents).unwrap();
            #[cfg(unix)]
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            path
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_credentials_and_preserves_password_colons() {
        assert_eq!(
            parse_credentials("alice:one:two\r\n").unwrap(),
            ("alice".to_string(), "one:two".to_string())
        );
    }

    #[test]
    fn rejects_invalid_credentials() {
        for value in [
            "",
            "alice",
            ":password",
            "alice:",
            "alice:pass\nbob:pass",
            "alice:pass\n\n",
        ] {
            assert!(parse_credentials(value).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn reads_regular_protected_credentials_file() {
        let directory = TestDirectory::new();
        let path = directory.write_secret("credentials", "alice:secret\n");

        assert_eq!(
            read_credentials(&path).unwrap(),
            ("alice".to_string(), "secret".to_string())
        );
    }

    #[test]
    fn rejects_directory() {
        let directory = TestDirectory::new();
        assert!(read_credentials(&directory.0)
            .unwrap_err()
            .contains("regular file"));
    }

    #[test]
    fn rejects_oversized_file() {
        let directory = TestDirectory::new();
        let path = directory.write_secret("credentials", "alice:secret");
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_SECRET_FILE_SIZE + 1)
            .unwrap();

        assert!(read_credentials(&path)
            .unwrap_err()
            .contains("1 MiB size limit"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink() {
        let directory = TestDirectory::new();
        let target = directory.write_secret("target", "alice:secret");
        let link = directory.0.join("credentials");
        symlink(target, &link).unwrap();

        assert!(read_credentials(&link)
            .unwrap_err()
            .contains("symbolic link"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_group_or_other_permissions() {
        let directory = TestDirectory::new();
        let path = directory.write_secret("credentials", "alice:secret");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        assert!(read_credentials(&path).unwrap_err().contains("chmod 600"));
    }
}
