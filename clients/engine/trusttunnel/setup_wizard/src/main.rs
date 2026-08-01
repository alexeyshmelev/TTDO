use crate::settings::{Endpoint, Settings};
use crate::user_interaction::{ask_for_input, checked_overwrite, select_index};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::ops::Not;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

mod composer;
mod secret_input;
mod settings;
mod template_settings;
mod user_interaction;
mod version;

const MODE_PARAM_NAME: &str = "mode";
const MODE_NON_INTERACTIVE: &str = "non-interactive";
const MODE_INTERACTIVE: &str = "interactive";
const ENDPOINT_ADDRESS_PARAM_NAME: &str = "address";
const HOSTNAME_PARAM_NAME: &str = "host";
const CREDENTIALS_FILE_PARAM_NAME: &str = "creds_file";
const CERTIFICATE_FILE_PARAM_NAME: &str = "cert";
const SETTINGS_FILE_PARAM_NAME: &str = "settings";
const ENDPOINT_CONFIG_PARAM_NAME: &str = "endpoint_config";
const CUSTOM_SNI_PARAM_NAME: &str = "custom_sni";
const DEEPLINK_FILE_PARAM_NAME: &str = "deeplink_file";

#[derive(Clone, Copy, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub enum Mode {
    NonInteractive,
    Interactive,
}

static MODE: Mutex<Mode> = Mutex::new(Mode::Interactive);
static SECRET_FILE_NONCE: AtomicU64 = AtomicU64::new(0);

pub fn get_mode() -> Mode {
    *MODE.lock().unwrap()
}

#[derive(Default, Clone)]
pub struct PredefinedParameters {
    endpoint_addresses: Option<Vec<String>>,
    hostname: Option<String>,
    custom_sni: Option<String>,
    credentials: Option<(String, String)>,
    certificate: Option<String>,
    endpoint_config: Option<String>,
    settings_file: Option<String>,
    pub deeplink: Option<String>,
}

impl PredefinedParameters {
    pub fn new(args: &clap::ArgMatches) -> Result<PredefinedParameters, String> {
        Ok(PredefinedParameters {
            endpoint_addresses: args
                .get_many::<String>(ENDPOINT_ADDRESS_PARAM_NAME)
                .map(Iterator::cloned)
                .map(Iterator::collect),
            hostname: args.get_one::<String>(HOSTNAME_PARAM_NAME).cloned(),
            credentials: args
                .get_one::<String>(CREDENTIALS_FILE_PARAM_NAME)
                .map(|path| secret_input::read_credentials(Path::new(path)))
                .transpose()?,
            custom_sni: args.get_one::<String>(CUSTOM_SNI_PARAM_NAME).cloned(),
            certificate: args.get_one::<String>(CERTIFICATE_FILE_PARAM_NAME).cloned(),
            endpoint_config: args.get_one::<String>(ENDPOINT_CONFIG_PARAM_NAME).cloned(),
            settings_file: args.get_one::<String>(SETTINGS_FILE_PARAM_NAME).cloned(),
            deeplink: args
                .get_one::<String>(DEEPLINK_FILE_PARAM_NAME)
                .map(|path| secret_input::read_deeplink(Path::new(path)))
                .transpose()?,
        })
    }
}

lazy_static::lazy_static! {
    pub static ref PREDEFINED_PARAMS: Mutex<PredefinedParameters> = Mutex::default();
}

pub fn get_predefined_params() -> MutexGuard<'static, PredefinedParameters> {
    PREDEFINED_PARAMS.lock().unwrap()
}

fn write_secret_file(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
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

fn main() {
    let mut command = clap::Command::new("TrustTunnel CLI Client Setup Wizard")
        .version(version::VERSION)
        .about("Generate configuration files for TrustTunnel CLI Client")
        .long_about(r#"Generate configuration files for TrustTunnel CLI Client.

TYPICAL WORKFLOW:
1. Get the endpoint config file from your TrustTunnel endpoint server
   (generated via: ./trusttunnel_endpoint vpn.toml hosts.toml -c <user> -a <ip> -f toml)
2. Run: ./setup_wizard --mode non-interactive --endpoint_config <config> --settings trusttunnel_client.toml
3. Start client: ./trusttunnel_client -c trusttunnel_client.toml

Alternatively, import from a deep-link URI:
  ./setup_wizard --mode non-interactive --deeplink-file deeplink.txt --settings trusttunnel_client.toml

For advanced settings (vpn_mode, killswitch, DNS upstreams, exclusions),
edit the generated TOML file directly. See README.md for all options."#)
        .after_help(r#"EXAMPLES:
    # Interactive mode (guided setup):
    ./setup_wizard

    # Non-interactive with endpoint config:
    ./setup_wizard -m non-interactive -e endpoint_config.toml --settings client.toml

    # Non-interactive with deep-link URI:
    ./setup_wizard -m non-interactive --deeplink-file deeplink.txt --settings client.toml

    # Non-interactive with manual parameters:
    ./setup_wizard -m non-interactive \
        -a 192.168.1.100:443 \
        -n vpn.example.com \
        --creds-file credentials.txt \
        --cert server.pem \
        --settings client.toml
"#)
        .args(&[
            clap::Arg::new(MODE_PARAM_NAME)
                .short('m')
                .long("mode")
                .action(clap::ArgAction::Set)
                .value_parser([MODE_INTERACTIVE, MODE_NON_INTERACTIVE])
                .default_value(MODE_INTERACTIVE)
                .help(format!(r#"Available wizard running modes:
    * {MODE_INTERACTIVE} - set up only the essential without deep diving into details
    * {MODE_NON_INTERACTIVE} - prepare the setup without interacting with a user,
        requires some parameters set up via command-line arguments
"#)),
            clap::Arg::new(ENDPOINT_ADDRESS_PARAM_NAME)
                .short('a')
                .long("address")
                .action(clap::ArgAction::Append)
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .help(format!(r#"{}.
Values of each parameter occurence are gathered into a list."#,
                              Endpoint::doc_addresses())),
            clap::Arg::new(HOSTNAME_PARAM_NAME)
                .short('n')
                .long("hostname")
                .action(clap::ArgAction::Set)
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .help(format!("{}.", Endpoint::doc_hostname())),
            clap::Arg::new(CREDENTIALS_FILE_PARAM_NAME)
                .short('c')
                .long("creds-file")
                .action(clap::ArgAction::Set)
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .help("Path to an owner-only regular file containing one <username>:<password> line."),
            clap::Arg::new(CERTIFICATE_FILE_PARAM_NAME)
                .long("cert")
                .action(clap::ArgAction::Set)
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .help("Path to a endpoint's certificate file."),
            clap::Arg::new(SETTINGS_FILE_PARAM_NAME)
                .long("settings")
                .action(clap::ArgAction::Set)
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .required_if_eq(MODE_PARAM_NAME, MODE_NON_INTERACTIVE)
                .help(r#"Path to store the library settings file.
Required in non-interactive mode."#),
            clap::Arg::new(CUSTOM_SNI_PARAM_NAME)
                .long("custom_sni")
                .action(clap::ArgAction::Set)
                .help(format!("{}.", Endpoint::doc_custom_sni())),
            clap::Arg::new(ENDPOINT_CONFIG_PARAM_NAME)
                .long("endpoint_config")
                .short('e')
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .conflicts_with("separate_options")
                .conflicts_with(DEEPLINK_FILE_PARAM_NAME)
                .help(format!("Path to an owner-only regular client-config file generated on the endpoint.\nConflicts with --{}, --{}, --{}, --{}", HOSTNAME_PARAM_NAME, CREDENTIALS_FILE_PARAM_NAME, ENDPOINT_ADDRESS_PARAM_NAME, DEEPLINK_FILE_PARAM_NAME)),
            clap::Arg::new(DEEPLINK_FILE_PARAM_NAME)
                .long("deeplink-file")
                .short('d')
                .value_parser(clap::builder::NonEmptyStringValueParser::new())
                .conflicts_with("separate_options")
                .conflicts_with(ENDPOINT_CONFIG_PARAM_NAME)
                .help(format!("Path to an owner-only regular file containing a tt:// deep-link URI.\nConflicts with --endpoint_config and manual options (--{}, --{}, --{}).", ENDPOINT_ADDRESS_PARAM_NAME, HOSTNAME_PARAM_NAME, CREDENTIALS_FILE_PARAM_NAME)),
        ])
        .group(
            clap::ArgGroup::new("separate_options")
                .args([HOSTNAME_PARAM_NAME, CREDENTIALS_FILE_PARAM_NAME, ENDPOINT_ADDRESS_PARAM_NAME, CERTIFICATE_FILE_PARAM_NAME])
                .multiple(true)
                .requires_all([HOSTNAME_PARAM_NAME, CREDENTIALS_FILE_PARAM_NAME, ENDPOINT_ADDRESS_PARAM_NAME])
        );
    let args = command.clone().get_matches();

    *MODE.lock().unwrap() = match args
        .get_one::<String>(MODE_PARAM_NAME)
        .map(String::as_str)
        .unwrap_or(MODE_INTERACTIVE)
    {
        MODE_NON_INTERACTIVE => Mode::NonInteractive,
        MODE_INTERACTIVE => Mode::Interactive,
        _ => unreachable!(),
    };

    if get_mode() == Mode::Interactive && !std::io::stdin().is_terminal() {
        eprintln!("Error: Interactive mode requires a terminal (TTY).");
        eprintln!("Please run setup_wizard from a terminal, or use non-interactive mode:");
        eprintln!("  {} --help", std::env::args().next().unwrap_or_default());
        std::process::exit(1);
    }

    if get_mode() == Mode::NonInteractive
        && !(args.contains_id(ENDPOINT_CONFIG_PARAM_NAME)
            || args.contains_id(HOSTNAME_PARAM_NAME)
            || args.contains_id(DEEPLINK_FILE_PARAM_NAME))
    {
        command
            .error(
                clap::error::ErrorKind::MissingRequiredArgument,
                r#"Additional arguments required for non-interactive mode

Must be provided either:
1. All required options separately:
   --address <address> --hostname <host> --creds-file <path>

OR
2. A configuration file generated on endpoint:
   --endpoint_config <endpoint_config>

OR
3. A deep-link URI:
   --deeplink-file <path>

Note: Cannot mix these variants"#,
            )
            .exit();
    }

    *PREDEFINED_PARAMS.lock().unwrap() = PredefinedParameters::new(&args).unwrap_or_else(|error| {
        command
            .error(clap::error::ErrorKind::InvalidValue, error)
            .exit()
    });

    (get_mode() == Mode::Interactive).then(|| println!("Welcome to the setup wizard"));

    let settings_path = {
        #[allow(clippy::large_enum_variant)]
        enum Action {
            UseExisting { path: String },
            ModifyAndOverwrite { path: String, settings: Settings },
            MakeFromScratch,
        }

        let action = if let Some((path, settings)) = get_mode()
            .eq(&Mode::NonInteractive)
            .not()
            .then(|| find_existent_settings::<Settings>("."))
            .flatten()
        {
            let selection = select_index(
                format!("Found existing settings: {path}."),
                &["Use it", "Modify and overwrite", "Make new from scratch"],
                Some(0),
            );
            match selection {
                0 => Action::UseExisting { path },
                1 => Action::ModifyAndOverwrite { path, settings },
                2 => Action::MakeFromScratch,
                _ => unreachable!("{:?}", selection),
            }
        } else {
            Action::MakeFromScratch
        };
        match action {
            Action::UseExisting { path } => path,
            Action::ModifyAndOverwrite { path, settings } => {
                (get_mode() == Mode::Interactive).then(|| println!("Let's build the settings"));
                let settings = settings::build(Some(&settings));
                println!("The settings are successfully built");

                let doc = composer::compose_document(Some(&path), &settings);
                write_secret_file(&path, doc.to_string())
                    .expect("Couldn't write the settings to a file");

                path
            }
            Action::MakeFromScratch => {
                (get_mode() == Mode::Interactive).then(|| println!("Let's build the settings"));
                let settings = settings::build(None);
                println!("The settings are successfully built");

                let path = ask_for_input::<String>(
                    "Path to a file to store the settings",
                    get_predefined_params()
                        .settings_file
                        .clone()
                        .or(Some("trusttunnel_client.toml".into())),
                );
                if checked_overwrite(&path, "Overwrite the existing settings file?") {
                    let doc = composer::compose_document(None, &settings);
                    write_secret_file(&path, doc.to_string())
                        .expect("Couldn't write the settings to a file");
                } else {
                    println!("Config file was not saved.");
                    return;
                }
                path
            }
        }
    };

    print_completion_message(&settings_path);
}

fn print_completion_message(settings_path: &str) {
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("                    Setup Complete!");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("Configuration file created:");
    println!("  • {}  - Client settings", settings_path);
    println!();
    println!("───────────────────────────────────────────────────────────────");
    println!("                      Next Steps");
    println!("───────────────────────────────────────────────────────────────");
    println!();
    println!("1. Start the client:");
    #[cfg(target_os = "windows")]
    println!("   trusttunnel_client -c {}", settings_path);
    #[cfg(not(target_os = "windows"))]
    println!("   sudo ./trusttunnel_client -c {}", settings_path);
    println!();
    println!("2. For advanced settings (exclusions, DNS, kill switch), edit:");
    println!("   {}", settings_path);
    println!();
    #[cfg(target_os = "windows")]
    println!("Note: For TUN mode, wintun.dll must be in the same directory or PATH.");
    #[cfg(target_os = "windows")]
    println!();
    println!("See clients/engine/README.md in the source repository for documentation.");
    println!();
    println!("Run `trusttunnel_client -h` for all available options.");
}

fn find_existent_settings<T: serde::de::DeserializeOwned>(path: &str) -> Option<(String, T)> {
    fs::read_dir(path)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .metadata()
                .map(|meta| meta.is_file())
                .unwrap_or_default()
        })
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|fname| fs::read_to_string(&fname).ok().zip(Some(fname)))
        .find_map(|(content, fname)| Some(fname).zip(toml::from_str::<T>(&content).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NONCE: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "trusttunnel-client-secret-test-{}-{}",
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
    fn secret_settings_overwrite_existing_contents() {
        let directory = TestDirectory::new();
        let path = directory.0.join("trusttunnel_client.toml");

        write_secret_file(&path, "first").unwrap();
        write_secret_file(&path, "second").unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "second");
    }

    #[cfg(unix)]
    #[test]
    fn secret_settings_are_restricted_after_overwrite() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new();
        let path = directory.0.join("trusttunnel_client.toml");
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
    fn secret_settings_replace_symlink_without_clobbering_target() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let target = directory.0.join("target");
        let path = directory.0.join("trusttunnel_client.toml");
        fs::write(&target, "unchanged").unwrap();
        symlink(&target, &path).unwrap();

        write_secret_file(&path, "settings").unwrap();

        assert_eq!(fs::read_to_string(target).unwrap(), "unchanged");
        assert_eq!(fs::read_to_string(&path).unwrap(), "settings");
        assert!(!fs::symlink_metadata(path).unwrap().file_type().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn secret_settings_refuse_non_regular_destination() {
        use std::os::unix::fs::FileTypeExt;
        use std::os::unix::net::UnixDatagram;

        let directory = TestDirectory::new();
        let path = directory.0.join("trusttunnel_client.toml");
        let _socket = UnixDatagram::bind(&path).unwrap();

        let error = write_secret_file(&path, "settings").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_socket());
    }
}
