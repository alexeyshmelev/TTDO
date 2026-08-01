use crate::acme::{
    issue_certificate, validate_domain, validate_email, AcmeConfig, ChallengeMethod, IssuedCert,
};
use crate::user_interaction::{ask_for_agreement, ask_for_input, checked_overwrite, select_index};
use crate::Mode;
use chrono::{Datelike, Duration, Local};
use rcgen::DnType;
use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;
use trusttunnel::settings::{TlsHostInfo, TlsHostsSettings};
use trusttunnel::utils;
use trusttunnel::utils::Either;
use x509_parser::extensions::GeneralName;

const DEFAULT_CERTIFICATE_DURATION_DAYS: u64 = 365;
const DEFAULT_CERTIFICATE_FOLDER: &str = "certs";
const DEFAULT_HOSTNAME: &str = "vpn.endpoint";

pub struct TlsHostsSettingsResult {
    pub settings: TlsHostsSettings,
    pub cert_path: String,
    pub key_path: String,
}

pub fn build() -> TlsHostsSettingsResult {
    loop {
        if let Some(cert) = build_with_runtime() {
            let cert_path = cert.cert_path.clone();
            let key_path = cert.key_path.clone();
            return TlsHostsSettingsResult {
                settings: build_settings_from_cert(cert),
                cert_path,
                key_path,
            };
        }
        // In non-interactive mode, we can't retry
        if crate::get_mode() == Mode::NonInteractive {
            panic!("Certificate is required in non-interactive mode");
        }
        println!("\nNo certificate was created. Let's try again.\n");
    }
}

pub fn build_with_runtime() -> Option<Cert> {
    // Check for non-interactive mode with ACME parameters
    if crate::get_mode() == Mode::NonInteractive {
        // Check if Let's Encrypt is requested via CLI
        if let Some(ref cert_type) = crate::get_predefined_params().cert_type {
            if cert_type == "provided" {
                return load_provided_cert_noninteractive();
            }
            if cert_type == "letsencrypt" {
                return generate_letsencrypt_cert_noninteractive();
            }
        }
        // Default to self-signed for non-interactive
        return generate_cert();
    }

    // Interactive mode
    lookup_existent_cert()
        .and_then(|x| {
            ask_for_agreement(&format!("Use an existing certificate? {:?}", x)).then_some(x)
        })
        .or_else(|| {
            let options = [
                "Issue a Let's Encrypt certificate (requires a public domain)",
                "Generate a self-signed certificate",
                "Provide path to existing certificate",
            ];

            let selection = select_index(
                "How would you like to create a certificate?",
                &options,
                Some(0),
            );

            match selection {
                0 => generate_letsencrypt_cert(),
                1 => generate_cert(),
                2 => ask_for_existing_cert(),
                _ => unreachable!(),
            }
        })
}

fn ask_for_existing_cert() -> Option<Cert> {
    let pair = ask_for_input::<String>(
        "Path to certificate file(s):\n  \
         - Single file containing both cert and key: /path/to/combined.pem\n  \
         - Separate files: /path/to/cert.pem /path/to/key.pem\n",
        None,
    );

    let mut iter = pair.splitn(2, char::is_whitespace);
    let x = match (iter.next().unwrap(), iter.next()) {
        (a, None) => Either::Left(a),
        (a, Some(b)) => Either::Right((a, b)),
    };

    let x = parse_cert(x);
    if x.is_none() {
        println!("Couldn't parse the provided key/certificate pair");
    }
    x
}

fn build_settings_from_cert(cert: Cert) -> TlsHostsSettings {
    let hostname = cert.common_name.clone();
    let allowed_sni = ask_for_alternative_snis();

    TlsHostsSettings::builder()
        .main_hosts(vec![TlsHostInfo {
            hostname: hostname.clone(),
            cert_chain_path: cert.cert_path.clone(),
            private_key_path: cert.key_path.clone(),
            allowed_sni,
        }])
        .build()
        .expect("Couldn't build TLS hosts settings")
}

#[derive(Debug, Clone)]
pub struct Cert {
    common_name: String,
    #[allow(dead_code)] // needed only for logging
    alt_names: Vec<String>,
    #[allow(dead_code)] // needed only for logging
    expiration_date: String,
    cert_path: String,
    key_path: String,
}

fn lookup_existent_cert() -> Option<Cert> {
    let files = fs::read_dir(DEFAULT_CERTIFICATE_FOLDER)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .metadata()
                .map(|meta| meta.is_file())
                .unwrap_or_default()
        })
        .filter_map(|entry| entry.path().to_str().map(String::from))
        .collect::<Vec<_>>();

    let cert_key_pair = match files.as_slice() {
        [a] => Either::Left(a.as_str()),
        [a, b] => Either::Right((a.as_str(), b.as_str())),
        _ => return None,
    };

    parse_cert(cert_key_pair)
}

fn print_cert_error(path: &str, error: &std::io::Error) {
    let message = match error.kind() {
        std::io::ErrorKind::PermissionDenied => {
            format!("Permission denied: cannot read '{}'", path)
        }
        std::io::ErrorKind::NotFound => {
            format!("File not found: '{}'", path)
        }
        std::io::ErrorKind::InvalidInput => {
            format!("Invalid certificate or key format in '{}': {}", path, error)
        }
        _ => {
            format!("Failed to read '{}': {}", path, error)
        }
    };
    eprintln!("Error: {}", message);
}

fn parse_cert(cert: Either<&str, (&str, &str)>) -> Option<Cert> {
    let (chain, cert_path, key_path) = cert.map(
        |pair| {
            Some((
                utils::load_private_key(pair)
                    .and_then(|_| utils::load_certs(pair))
                    .map_err(|e| print_cert_error(pair, &e))
                    .ok()?,
                pair,
                pair,
            ))
        },
        |(a, b)| match (
            utils::load_certs(a),
            utils::load_private_key(b),
            utils::load_certs(b),
            utils::load_private_key(a),
        ) {
            (Ok(chain), Ok(_), _, _) => Some((chain, a, b)),
            (_, _, Ok(chain), Ok(_)) => Some((chain, b, a)),
            (Err(e), _, _, _) => {
                print_cert_error(a, &e);
                None
            }
            (_, Err(e), _, _) => {
                print_cert_error(b, &e);
                None
            }
        },
    )?;

    let cert = x509_parser::parse_x509_certificate(chain.first()?.as_ref())
        .ok()?
        .1;
    Some(Cert {
        common_name: cert.validity.is_valid().then(|| {
            let x = cert.subject.to_string();
            x.as_str()
                .strip_prefix("CN=")
                .map(String::from)
                .unwrap_or(x)
        })?,
        alt_names: cert
            .subject_alternative_name()
            .ok()
            .flatten()
            .map(|x| {
                x.value
                    .general_names
                    .iter()
                    .map(GeneralName::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        expiration_date: cert.validity.not_after.to_string(),
        cert_path: cert_path.into(),
        key_path: key_path.into(),
    })
}

fn generate_cert() -> Option<Cert> {
    let (common_name, alt_names) = {
        println!("Let's generate a self-signed certificate.");
        let name = crate::get_predefined_params()
            .hostname
            .clone()
            .unwrap_or_else(|| {
                ask_for_input::<String>(
                    "Endpoint hostname (used for serving TLS connections)",
                    Some(DEFAULT_HOSTNAME.into()),
                )
            });
        (name.clone(), vec![name.clone(), format!("*.{}", name)])
    };
    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
        .expect("Failed to generate key pair");

    let mut params = rcgen::CertificateParams::new(alt_names.clone()).unwrap();
    let now = chrono::Local::now();
    let end_date = now
        .checked_add_days(chrono::Days::new(DEFAULT_CERTIFICATE_DURATION_DAYS))
        .unwrap();
    params.not_before = rcgen::date_time_ymd(now.year(), now.month() as u8, now.day() as u8);
    params.not_after = rcgen::date_time_ymd(
        end_date.year(),
        end_date.month() as u8,
        end_date.day() as u8,
    );
    params
        .distinguished_name
        .push(DnType::CommonName, &common_name);

    let cert = params
        .self_signed(&key_pair)
        .expect("Failed to generate self-signed cert");
    let cert_path = format!("{DEFAULT_CERTIFICATE_FOLDER}/cert.pem");
    if !checked_overwrite(&cert_path, "Overwrite the existing certificate file?") {
        return None;
    }

    let key_path = format!("{DEFAULT_CERTIFICATE_FOLDER}/key.pem");
    if !checked_overwrite(&key_path, "Overwrite the existing private key file?") {
        return None;
    }

    write_certificate_pair(
        Path::new(&cert_path),
        Path::new(&key_path),
        cert.pem().as_bytes(),
        key_pair.serialize_pem().as_bytes(),
    )
    .expect("Couldn't write the certificate and private key");
    println!("The generated certificate is stored in file: {}", cert_path);
    println!("The generated private key is stored in file: {}", key_path);

    Some(Cert {
        common_name,
        alt_names,
        expiration_date: end_date.to_string(),
        cert_path,
        key_path,
    })
}

fn save_issued_cert(issued: IssuedCert, interactive: bool) -> Option<Cert> {
    let cert_path = format!("{}/cert.pem", DEFAULT_CERTIFICATE_FOLDER);
    let key_path = format!("{}/key.pem", DEFAULT_CERTIFICATE_FOLDER);

    if interactive {
        if !checked_overwrite(&cert_path, "Overwrite the existing certificate file?") {
            return None;
        }
        if !checked_overwrite(&key_path, "Overwrite the existing private key file?") {
            return None;
        }
    }

    write_certificate_pair(
        Path::new(&cert_path),
        Path::new(&key_path),
        issued.cert_pem.as_bytes(),
        issued.key_pem.as_bytes(),
    )
    .expect("Couldn't write the certificate and private key");
    println!("Certificate saved to: {}", cert_path);
    println!("Private key saved to: {}", key_path);

    let expiration_date = parse_cert_expiration(&issued.cert_pem).unwrap_or_else(|| {
        Local::now()
            .checked_add_signed(Duration::days(90))
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "90 days from now".to_string())
    });

    Some(Cert {
        common_name: issued.domain.clone(),
        alt_names: vec![issued.domain],
        expiration_date,
        cert_path,
        key_path,
    })
}

fn write_certificate_pair(
    cert_path: &Path,
    key_path: &Path,
    cert_pem: &[u8],
    key_pem: &[u8],
) -> io::Result<()> {
    let cert_directory = cert_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "certificate path has no parent",
        )
    })?;
    let key_directory = key_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "private key path has no parent",
        )
    })?;
    ensure_private_directory(cert_directory)?;
    if key_directory != cert_directory {
        ensure_private_directory(key_directory)?;
    }

    if cert_path == key_path {
        let mut combined = Vec::with_capacity(cert_pem.len() + key_pem.len());
        combined.extend_from_slice(cert_pem);
        combined.extend_from_slice(key_pem);
        return crate::library_settings::write_secret_file(cert_path, combined);
    }

    crate::library_settings::write_secret_file(key_path, key_pem)?;
    crate::library_settings::write_secret_file(cert_path, cert_pem)
}

fn ensure_private_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "certificate directory must not be a symlink",
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "certificate directory path is not a directory",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            #[cfg(unix)]
            {
                let mut builder = fs::DirBuilder::new();
                builder.recursive(true).mode(0o700).create(path)?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(path)?;
        }
        Err(error) => return Err(error),
    }

    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "certificate directory is not a real directory",
        ));
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn generate_letsencrypt_cert() -> Option<Cert> {
    println!("Let's issue a Let's Encrypt certificate.");

    // Get domain name
    let domain: String = loop {
        let domain: String =
            ask_for_input("Enter your domain name (must be publicly accessible)", None);
        if validate_domain(&domain) {
            break domain;
        }
        println!(
            "Invalid domain format. Please enter a valid domain name (e.g., vpn.example.com)."
        );
    };

    // Get email address
    let email: String = loop {
        let email: String = ask_for_input(
            "Enter your email address (for Let's Encrypt notifications)",
            None,
        );
        if validate_email(&email) {
            break email;
        }
        println!("Invalid email format. Please try again.");
    };

    // Select challenge method
    let challenge_options = [
        "HTTP-01 (requires port 80 accessible from internet)",
        "DNS-01 (requires adding a TXT record to your DNS)",
    ];
    let challenge_selection = select_index("Select challenge method", &challenge_options, Some(0));
    let challenge_method = match challenge_selection {
        0 => ChallengeMethod::Http01,
        1 => ChallengeMethod::Dns01,
        _ => unreachable!(),
    };

    // Ask about staging environment
    let use_staging = ask_for_agreement(
        "Use Let's Encrypt staging environment for testing? (recommended for first attempt)",
    );

    if use_staging {
        println!(
            "\n[WARNING] Using staging environment. Certificate will NOT be trusted by browsers."
        );
        println!("   Run again without staging for a production certificate.\n");
    }

    let config = AcmeConfig {
        domain,
        email,
        challenge_method,
        use_staging,
    };

    // Run the async ACME flow
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let result = runtime.block_on(issue_certificate(config));

    match result {
        Ok(issued) => save_issued_cert(issued, true),
        Err(ref e) => {
            println!("\n[ERROR] Failed to issue Let's Encrypt certificate: {}", e);
            println!("\nPossible solutions:");
            match e {
                crate::acme::AcmeError::PortInUse(_) => {
                    println!("  • Stop any service using port 80, or");
                    println!("  • Use DNS-01 challenge instead");
                }
                crate::acme::AcmeError::ChallengeFailed(_) => {
                    println!("  • Verify your domain resolves to this server's IP");
                    println!("  • Check firewall allows inbound HTTP (port 80)");
                    println!("  • For DNS-01, ensure TXT record is correct and propagated");
                }
                _ => {
                    println!("  • Check your internet connection");
                    println!("  • Try using the staging environment first");
                }
            }

            if ask_for_agreement("Would you like to generate a self-signed certificate instead?") {
                generate_cert()
            } else {
                None
            }
        }
    }
}

fn generate_letsencrypt_cert_noninteractive() -> Option<Cert> {
    let predefined = crate::get_predefined_params();

    let domain = predefined
        .hostname
        .clone()
        .expect("Hostname is required for Let's Encrypt in non-interactive mode");
    if !validate_domain(&domain) {
        eprintln!("Invalid domain format: {}", domain);
        return None;
    }
    let email = predefined
        .acme_email
        .clone()
        .expect("ACME email is required for Let's Encrypt in non-interactive mode");
    if !validate_email(&email) {
        eprintln!("Invalid email format: {}", email);
        return None;
    }
    let challenge_method = predefined
        .acme_challenge
        .clone()
        .map(|s| {
            let method = s.parse::<ChallengeMethod>()
                .expect("Invalid challenge method");
            if method == ChallengeMethod::Dns01 {
                panic!("DNS-01 challenge is not supported in non-interactive mode (requires manual DNS record confirmation)");
            }
            method
        })
        .unwrap_or(ChallengeMethod::Http01);
    let use_staging = predefined.acme_staging;

    drop(predefined);

    if use_staging {
        println!("[WARNING] Using Let's Encrypt staging environment");
    }

    let config = AcmeConfig {
        domain,
        email,
        challenge_method,
        use_staging,
    };

    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let result = runtime.block_on(issue_certificate(config));

    match result {
        Ok(issued) => save_issued_cert(issued, false),
        Err(e) => {
            eprintln!("Failed to issue Let's Encrypt certificate: {}", e);
            None
        }
    }
}

fn load_provided_cert_noninteractive() -> Option<Cert> {
    let predefined = crate::get_predefined_params();
    let cert_chain_path = predefined
        .cert_chain_path
        .clone()
        .expect("Certificate chain path is required for provided cert type");
    let cert_key_path = predefined
        .cert_key_path
        .clone()
        .expect("Certificate key path is required for provided cert type");
    drop(predefined);

    let cert = parse_cert(Either::Right((&cert_chain_path, &cert_key_path)));
    if cert.is_none() {
        eprintln!(
            "Failed to load provided certificate and key from '{}' and '{}'",
            cert_chain_path, cert_key_path
        );
    }

    cert
}

fn parse_cert_expiration(cert_pem: &str) -> Option<String> {
    let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes()).ok()?;
    let (_, cert) = x509_parser::parse_x509_certificate(&pem.contents).ok()?;
    let not_after = cert.validity.not_after.to_datetime();
    Some(format!(
        "{:04}-{:02}-{:02}",
        not_after.year(),
        not_after.month(),
        not_after.day()
    ))
}

fn ask_for_alternative_snis() -> Vec<String> {
    if crate::get_mode() == Mode::NonInteractive {
        return vec![];
    }

    if !ask_for_agreement("Do you want to configure alternative SNIs?") {
        return vec![];
    }

    let input: String = ask_for_input(
        "Enter alternative SNIs (comma-separated)",
        Some(String::new()),
    );

    if input.trim().is_empty() {
        return vec![];
    }

    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            static NONCE: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "trusttunnel-certificate-test-{}-{}",
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
    fn certificate_pair_overwrites_existing_contents() {
        let directory = TestDirectory::new();
        let cert_directory = directory.0.join("certs");
        let cert_path = cert_directory.join("cert.pem");
        let key_path = cert_directory.join("key.pem");

        write_certificate_pair(&cert_path, &key_path, b"old cert", b"old key").unwrap();
        write_certificate_pair(&cert_path, &key_path, b"new cert", b"new key").unwrap();

        assert_eq!(fs::read(cert_path).unwrap(), b"new cert");
        assert_eq!(fs::read(key_path).unwrap(), b"new key");
    }

    #[cfg(unix)]
    #[test]
    fn certificate_pair_permissions_are_restricted() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new();
        let cert_directory = directory.0.join("certs");
        let cert_path = cert_directory.join("cert.pem");
        let key_path = cert_directory.join("key.pem");

        write_certificate_pair(&cert_path, &key_path, b"cert", b"key").unwrap();
        fs::set_permissions(&cert_directory, fs::Permissions::from_mode(0o777)).unwrap();
        fs::set_permissions(&cert_path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(&key_path, fs::Permissions::from_mode(0o666)).unwrap();
        write_certificate_pair(&cert_path, &key_path, b"new cert", b"new key").unwrap();

        assert_eq!(
            fs::metadata(cert_directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(cert_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(key_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn certificate_directory_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let target = directory.0.join("target");
        let cert_directory = directory.0.join("certs");
        fs::create_dir(&target).unwrap();
        symlink(&target, &cert_directory).unwrap();

        let error = write_certificate_pair(
            &cert_directory.join("cert.pem"),
            &cert_directory.join("key.pem"),
            b"cert",
            b"key",
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(fs::read_dir(target).unwrap().next().is_none());
    }
}
