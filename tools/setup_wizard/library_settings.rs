use crate::user_interaction::{
    ask_for_agreement, ask_for_input, ask_for_password, checked_overwrite, select_variant,
};
use crate::Mode;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use toml_edit::{ArrayOfTables, Item, Key, Table};
use trusttunnel::authentication::registry_based::Client;
use trusttunnel::settings::{
    Http1Settings, Http2Settings, ListenProtocolSettings, QuicSettings, Settings,
};

pub const DEFAULT_CREDENTIALS_PATH: &str = "credentials.toml";
pub const DEFAULT_RULES_PATH: &str = "rules.toml";

static SECRET_FILE_NONCE: AtomicU64 = AtomicU64::new(0);

pub struct Built {
    pub settings: Settings,
    pub credentials_path: String,
    pub rules_path: String,
}

pub fn build() -> Built {
    let builder = Settings::builder()
        .listen_address(
            crate::get_predefined_params()
                .listen_address
                .clone()
                .unwrap_or_else(|| {
                    ask_for_input(
                        &format!(
                            "{} (native: 0.0.0.0:443; Docker with 443:8443 mapping: 0.0.0.0:8443)",
                            Settings::doc_listen_address()
                        ),
                        Some(Settings::default_listen_address().to_string()),
                    )
                }),
        )
        .unwrap();
    let ipv6_available = resolve_ipv6_available(
        crate::get_mode(),
        crate::get_predefined_params().ipv6_available,
        || ask_for_agreement("Does this server have working outbound IPv6?"),
    );
    let builder = builder.ipv6_available(ipv6_available);

    // Collect credentials first, then build settings
    let (credentials_path, clients) = build_credentials();

    Built {
        settings: builder
            .listen_protocols(ListenProtocolSettings {
                http1: Some(Http1Settings::builder().build()),
                http2: Some(Http2Settings::builder().build()),
                quic: Some(QuicSettings::builder().build()),
            })
            .clients(clients)
            .build()
            .expect("Couldn't build the library settings"),
        credentials_path,
        rules_path: build_rules(),
    }
}

fn resolve_ipv6_available<F>(mode: Mode, predefined: bool, prompt: F) -> bool
where
    F: FnOnce() -> bool,
{
    match mode {
        Mode::NonInteractive => predefined,
        Mode::Interactive => prompt(),
    }
}

pub(crate) fn write_secret_file(
    path: impl AsRef<Path>,
    contents: impl AsRef<[u8]>,
) -> io::Result<()> {
    let path = path.as_ref();
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "secret file path has no file name",
        )
    })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));

    let (temporary_path, mut file) = (0..128)
        .find_map(|_| {
            let nonce = SECRET_FILE_NONCE.fetch_add(1, Ordering::Relaxed);
            let temporary_name = format!(
                ".{}.{}.{}.tmp",
                file_name.to_string_lossy(),
                std::process::id(),
                nonce
            );
            let temporary_path = parent.join(temporary_name);
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&temporary_path) {
                Ok(file) => Some(Ok((temporary_path, file))),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .transpose()?
        .ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "temporary file collision"))?;

    let write_result = (|| {
        #[cfg(unix)]
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.write_all(contents.as_ref())?;
        file.sync_all()
    })();
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }

    if let Err(error) = replace_file(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
    }
    Ok(())
}

#[cfg(unix)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "secret file destination must be a regular file or symbolic link",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(source, destination)
}

#[cfg(not(unix))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "secret file path is a directory",
            ));
        }
        Ok(_) => fs::remove_file(destination)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NONCE: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "trusttunnel-server-secret-test-{}-{}",
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
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn non_interactive_ipv6_defaults_to_disabled() {
        assert!(!resolve_ipv6_available(Mode::NonInteractive, false, || {
            panic!("non-interactive setup must not prompt")
        }));
    }

    #[test]
    fn non_interactive_ipv6_can_be_enabled() {
        assert!(resolve_ipv6_available(Mode::NonInteractive, true, || {
            panic!("non-interactive setup must not prompt")
        }));
    }

    #[test]
    fn interactive_ipv6_uses_prompt_answer() {
        assert!(resolve_ipv6_available(Mode::Interactive, false, || true));
    }

    #[test]
    fn secret_file_overwrites_existing_contents() {
        let directory = TestDirectory::new();
        let path = directory.0.join("credentials.toml");

        write_secret_file(&path, "first").unwrap();
        write_secret_file(&path, "second").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "second");
    }

    #[cfg(unix)]
    #[test]
    fn secret_file_is_restricted_after_overwrite() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new();
        let path = directory.0.join("credentials.toml");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();

        write_secret_file(&path, "new").unwrap();

        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn secret_file_replaces_symlink_without_clobbering_target() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let target = directory.0.join("target");
        let path = directory.0.join("credentials.toml");
        fs::write(&target, "unchanged").unwrap();
        symlink(&target, &path).unwrap();

        write_secret_file(&path, "credentials").unwrap();

        assert_eq!(fs::read_to_string(target).unwrap(), "unchanged");
        assert_eq!(fs::read_to_string(&path).unwrap(), "credentials");
        assert!(!fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn secret_file_refuses_non_regular_destination() {
        use std::os::unix::fs::FileTypeExt;
        use std::os::unix::net::UnixDatagram;

        let directory = TestDirectory::new();
        let path = directory.0.join("credentials.toml");
        let _socket = UnixDatagram::bind(&path).unwrap();

        let error = write_secret_file(&path, "credentials").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_socket());
    }
}

fn build_credentials() -> (String, Vec<Client>) {
    if crate::get_mode() != Mode::NonInteractive
        && check_file_exists(".", DEFAULT_CREDENTIALS_PATH)
        && ask_for_agreement(&format!(
            "Reuse the existing credentials file: {DEFAULT_CREDENTIALS_PATH}?"
        ))
    {
        let clients = read_credentials_file(DEFAULT_CREDENTIALS_PATH).unwrap_or_default();
        return (DEFAULT_CREDENTIALS_PATH.into(), clients);
    }

    let path = ask_for_input::<String>(
        "Path to the credentials file",
        Some(DEFAULT_CREDENTIALS_PATH.into()),
    );

    let users = build_user_list();

    if checked_overwrite(&path, "Overwrite the existing credentials file?") {
        write_secret_file(&path, compose_credentials_content(users.iter().cloned()))
            .expect("Couldn't write the credentials into a file");
        println!("The user credentials are written to the file: {}", path);
    }

    let clients = users
        .into_iter()
        .map(|(username, password)| Client {
            username,
            password,
            max_http2_conns: None,
            max_http3_conns: None,
        })
        .collect();

    (path, clients)
}

fn read_credentials_file(path: &str) -> Option<Vec<Client>> {
    let content = fs::read_to_string(path).ok()?;
    let doc: toml_edit::Document = content.parse().ok()?;
    let tables = doc.get("client")?.as_array_of_tables()?;
    Some(
        tables
            .iter()
            .filter_map(|t| {
                Some(Client {
                    username: t.get("username")?.as_str()?.to_string(),
                    password: t.get("password")?.as_str()?.to_string(),
                    max_http2_conns: t
                        .get("max_http2_conns")
                        .and_then(|v| v.as_integer())
                        .map(|v| v as u32),
                    max_http3_conns: t
                        .get("max_http3_conns")
                        .and_then(|v| v.as_integer())
                        .map(|v| v as u32),
                })
            })
            .collect(),
    )
}

fn build_rules() -> String {
    if crate::get_mode() != Mode::NonInteractive
        && check_file_exists(".", DEFAULT_RULES_PATH)
        && ask_for_agreement(&format!(
            "Reuse the existing rules file: {DEFAULT_RULES_PATH}?"
        ))
    {
        DEFAULT_RULES_PATH.into()
    } else {
        let path =
            ask_for_input::<String>("Path to the rules file", Some(DEFAULT_RULES_PATH.into()));

        if checked_overwrite(&path, "Overwrite the existing rules file?") {
            println!("Let's create connection filtering rules");
            let rules_config = crate::rules_settings::build();
            let rules_content = generate_rules_toml_content(&rules_config);
            fs::write(&path, rules_content).expect("Couldn't write the rules into a file");
            println!("The rules configuration is written to the file: {}", path);
        }

        path
    }
}

fn build_user_list() -> Vec<(String, String)> {
    if let Some(x) = crate::get_predefined_params().credentials.clone() {
        return vec![x];
    }

    let mut list = vec![(
        ask_for_input::<String>("Username", None),
        ask_for_password("Password"),
    )];

    loop {
        if "no" == select_variant("Add one more user?", &["yes", "no"], Some(1)) {
            break;
        }

        list.push((
            ask_for_input::<String>("Username", None),
            ask_for_password("Password"),
        ));
    }

    list
}

fn compose_credentials_content(clients: impl Iterator<Item = (String, String)>) -> String {
    let mut doc = toml_edit::Document::new();

    let x = clients
        .map(|(u, p)| {
            Table::from_iter(
                std::iter::once(("username", u)).chain(std::iter::once(("password", p))),
            )
        })
        .collect::<ArrayOfTables>();

    doc.insert_formatted(&Key::new("client"), Item::ArrayOfTables(x));

    doc.to_string()
}

fn generate_rules_toml_content(rules_config: &trusttunnel::rules::RulesConfig) -> String {
    let mut content = String::new();

    // Add header comments explaining the format
    content.push_str("# Rules configuration for VPN endpoint connection filtering\n");
    content.push_str("# \n");
    content.push_str("# This file defines filter rules for incoming connections.\n");
    content.push_str(
        "# Rules are evaluated in order, and the first matching rule's action is applied.\n",
    );
    content.push_str("# If no rules match, the connection is allowed by default.\n");
    content.push_str("#\n");
    content.push_str("# Each rule can specify:\n");
    content.push_str("# - cidr: IP address range in CIDR notation\n");
    content.push_str("# - client_random_prefix: Hex-encoded prefix of TLS client random data\n");
    content.push_str(
        "#   Can optionally include a mask in format \"prefix[/mask]\" for bitwise matching\n",
    );
    content.push_str("# - action: \"allow\" or \"deny\"\n");
    content.push_str("#\n");
    content.push_str("# All fields except 'action' are optional - if specified, all conditions must match for the rule to apply.\n");
    content.push_str("#\n");
    content.push_str("# client_random_prefix formats:\n");
    content.push_str("# 1. Simple prefix matching:\n");
    content.push_str("#    client_random_prefix = \"aabbcc\"\n");
    content.push_str("#    → matches client_random starting with 0xaabbcc\n");
    content.push_str("#\n");
    content.push_str("# 2. Bitwise matching with mask:\n");
    content.push_str("#    client_random_prefix = \"a0b0/f0f0\"\n");
    content.push_str("#    → prefix=a0b0, mask=f0f0\n");
    content.push_str(
        "#    → matches client_random where (client_random & 0xf0f0) == (0xa0b0 & 0xf0f0)\n",
    );
    content.push_str("#    → e.g., 0xa5b5, 0xa9bf match, but 0xb0b0, 0xa0c0 don't match\n\n");

    // Serialize the actual rules (usually empty)
    if !rules_config.rule.is_empty() {
        content.push_str(&toml::ser::to_string(rules_config).unwrap());
        content.push('\n');
    }

    content
}

fn check_file_exists(path: &str, name: &str) -> bool {
    match fs::read_dir(path) {
        Ok(x) => x
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .metadata()
                    .map(|meta| meta.is_file())
                    .unwrap_or_default()
            })
            .any(|entry| Ok(name) == entry.file_name().into_string().as_ref().map(String::as_str)),
        Err(_) => false,
    }
}
