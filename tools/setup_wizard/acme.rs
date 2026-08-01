use crate::acme_http_server::run_http01_challenge_server;
use crate::user_interaction::ask_for_agreement;
use instant_acme::{
    Account, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt, NewAccount, NewOrder,
    OrderStatus,
};
use rcgen::{CertificateParams, DistinguishedName, DnType};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeMethod {
    Http01,
    Dns01,
}

impl std::fmt::Display for ChallengeMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChallengeMethod::Http01 => write!(f, "http-01"),
            ChallengeMethod::Dns01 => write!(f, "dns-01"),
        }
    }
}

impl std::str::FromStr for ChallengeMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "http-01" | "http01" => Ok(ChallengeMethod::Http01),
            "dns-01" | "dns01" => Ok(ChallengeMethod::Dns01),
            _ => Err(format!("Unknown challenge method: {}", s)),
        }
    }
}

pub struct AcmeConfig {
    pub domain: String,
    pub email: String,
    pub challenge_method: ChallengeMethod,
    pub use_staging: bool,
}

pub struct IssuedCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub domain: String,
}

#[derive(Debug)]
pub enum AcmeError {
    AccountCreation(String),
    OrderCreation(String),
    Authorization(String),
    ChallengeNotFound(ChallengeType),
    ChallengeFailed(String),
    CsrGeneration(String),
    Finalization(String),
    CertificateDownload(String),
    Timeout(String),
    PortInUse(u16),
    PermissionDenied(u16),
    Other(String),
}

impl std::fmt::Display for AcmeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcmeError::AccountCreation(e) => write!(f, "Failed to create ACME account: {}", e),
            AcmeError::OrderCreation(e) => write!(f, "Failed to create certificate order: {}", e),
            AcmeError::Authorization(e) => write!(f, "Authorization failed: {}", e),
            AcmeError::ChallengeNotFound(t) => write!(f, "Challenge type {:?} not found", t),
            AcmeError::ChallengeFailed(e) => write!(f, "Challenge failed: {}", e),
            AcmeError::CsrGeneration(e) => write!(f, "Failed to generate CSR: {}", e),
            AcmeError::Finalization(e) => write!(f, "Failed to finalize order: {}", e),
            AcmeError::CertificateDownload(e) => write!(f, "Failed to download certificate: {}", e),
            AcmeError::Timeout(e) => write!(f, "Timeout: {}", e),
            AcmeError::PortInUse(port) => write!(f, "Port {} is already in use", port),
            AcmeError::PermissionDenied(port) => write!(
                f,
                "Permission denied: cannot bind to port {}. Try running as root or use sudo.",
                port
            ),
            AcmeError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for AcmeError {}

pub struct AcmeClient {
    account: Account,
}

impl AcmeClient {
    pub async fn new(email: &str, staging: bool) -> Result<Self, AcmeError> {
        let directory_url = if staging {
            LetsEncrypt::Staging.url()
        } else {
            LetsEncrypt::Production.url()
        };

        println!(
            "Creating ACME account with {} environment...",
            if staging { "staging" } else { "production" }
        );

        let (account, _credentials) = Account::create(
            &NewAccount {
                contact: &[&format!("mailto:{}", email)],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url,
            None,
        )
        .await
        .map_err(|e| AcmeError::AccountCreation(e.to_string()))?;

        println!("[OK] ACME account created successfully");
        Ok(Self { account })
    }

    pub async fn issue_cert(
        &self,
        domain: &str,
        challenge_method: ChallengeMethod,
    ) -> Result<IssuedCert, AcmeError> {
        println!("Requesting certificate for {}...", domain);

        // 1. Create order
        let identifiers = vec![Identifier::Dns(domain.to_string())];
        let mut order = self
            .account
            .new_order(&NewOrder {
                identifiers: &identifiers,
            })
            .await
            .map_err(|e| AcmeError::OrderCreation(e.to_string()))?;

        println!("[OK] Certificate order created");

        // 2. Get authorizations and complete challenges
        let authorizations = order
            .authorizations()
            .await
            .map_err(|e| AcmeError::Authorization(e.to_string()))?;

        for auth in &authorizations {
            match auth.status {
                AuthorizationStatus::Valid => {
                    println!("[OK] Authorization already valid");
                    continue;
                }
                AuthorizationStatus::Pending => {}
                status => {
                    return Err(AcmeError::Authorization(format!(
                        "Unexpected authorization status: {:?}",
                        status
                    )));
                }
            }

            let challenge = match challenge_method {
                ChallengeMethod::Http01 => auth
                    .challenges
                    .iter()
                    .find(|c| c.r#type == ChallengeType::Http01)
                    .ok_or(AcmeError::ChallengeNotFound(ChallengeType::Http01))?,
                ChallengeMethod::Dns01 => auth
                    .challenges
                    .iter()
                    .find(|c| c.r#type == ChallengeType::Dns01)
                    .ok_or(AcmeError::ChallengeNotFound(ChallengeType::Dns01))?,
            };

            let key_auth = order.key_authorization(challenge);

            match challenge_method {
                ChallengeMethod::Http01 => {
                    self.complete_http01_challenge(
                        &challenge.token,
                        key_auth.as_str(),
                        &mut order,
                        challenge,
                    )
                    .await?;
                }
                ChallengeMethod::Dns01 => {
                    let dns_value = key_auth.dns_value();
                    self.complete_dns01_challenge(domain, &dns_value, &mut order, challenge)
                        .await?;
                }
            }
        }

        // 3. Wait for order to be ready
        println!("Waiting for order to be ready...");
        let mut attempts = 0;
        let max_attempts = 30;
        loop {
            sleep(Duration::from_secs(2)).await;
            order
                .refresh()
                .await
                .map_err(|e| AcmeError::Other(e.to_string()))?;

            match order.state().status {
                OrderStatus::Ready => {
                    println!("[OK] Order is ready for finalization");
                    break;
                }
                OrderStatus::Invalid => {
                    return Err(AcmeError::Finalization("Order became invalid".to_string()));
                }
                OrderStatus::Valid => {
                    println!("[OK] Order is already valid");
                    break;
                }
                _ => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        return Err(AcmeError::Timeout(
                            "Timed out waiting for order to be ready".to_string(),
                        ));
                    }
                }
            }
        }

        // 4. Generate key pair and CSR
        println!("Generating certificate signing request...");

        let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| AcmeError::CsrGeneration(e.to_string()))?;

        let mut params = CertificateParams::new(vec![domain.to_string()])
            .map_err(|e| AcmeError::CsrGeneration(e.to_string()))?;
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(DnType::CommonName, domain);

        let csr = params
            .serialize_request(&key_pair)
            .map_err(|e| AcmeError::CsrGeneration(e.to_string()))?;

        let csr_der = csr.der().as_ref();

        // 5. Finalize order
        if order.state().status == OrderStatus::Ready {
            order
                .finalize(csr_der)
                .await
                .map_err(|e| AcmeError::Finalization(e.to_string()))?;
            println!("[OK] Order finalized");
        }

        let key_pem = key_pair.serialize_pem();

        // 6. Wait for certificate
        println!("Waiting for certificate...");
        let mut attempts = 0;
        loop {
            sleep(Duration::from_secs(2)).await;
            order
                .refresh()
                .await
                .map_err(|e| AcmeError::Other(e.to_string()))?;

            match order.state().status {
                OrderStatus::Valid => break,
                OrderStatus::Invalid => {
                    return Err(AcmeError::Finalization(
                        "Order became invalid after finalization".to_string(),
                    ));
                }
                _ => {
                    attempts += 1;
                    if attempts >= max_attempts {
                        return Err(AcmeError::Timeout(
                            "Timed out waiting for certificate".to_string(),
                        ));
                    }
                }
            }
        }

        // 7. Download certificate
        let cert_chain = order
            .certificate()
            .await
            .map_err(|e| AcmeError::CertificateDownload(e.to_string()))?
            .ok_or_else(|| AcmeError::CertificateDownload("No certificate returned".to_string()))?;

        println!("[OK] Certificate issued successfully!");

        Ok(IssuedCert {
            cert_pem: cert_chain,
            key_pem,
            domain: domain.to_string(),
        })
    }

    async fn complete_http01_challenge(
        &self,
        token: &str,
        key_authorization: &str,
        order: &mut instant_acme::Order,
        challenge: &instant_acme::Challenge,
    ) -> Result<(), AcmeError> {
        println!("Starting HTTP-01 challenge...");
        println!("  Token: {}", token);

        // Start HTTP server to serve the challenge
        let (shutdown_tx, server_handle) =
            run_http01_challenge_server(token.to_string(), key_authorization.to_string()).await?;

        // Notify ACME server that challenge is ready
        println!("Notifying ACME server that challenge is ready...");
        order
            .set_challenge_ready(&challenge.url)
            .await
            .map_err(|e| AcmeError::ChallengeFailed(e.to_string()))?;

        // Wait for validation
        println!("Waiting for ACME server to validate challenge...");
        let mut attempts = 0;
        let max_attempts = 30;
        loop {
            sleep(Duration::from_secs(2)).await;

            let auths = order
                .authorizations()
                .await
                .map_err(|e| AcmeError::Authorization(e.to_string()))?;

            let all_valid = auths.iter().all(|a| a.status == AuthorizationStatus::Valid);

            if all_valid {
                println!("[OK] HTTP-01 challenge completed successfully");
                break;
            }

            let any_invalid = auths
                .iter()
                .any(|a| a.status == AuthorizationStatus::Invalid);

            if any_invalid {
                let _ = shutdown_tx.send(());
                let _ = server_handle.await;
                return Err(AcmeError::ChallengeFailed(
                    "Authorization became invalid".to_string(),
                ));
            }

            attempts += 1;
            if attempts >= max_attempts {
                let _ = shutdown_tx.send(());
                let _ = server_handle.await;
                return Err(AcmeError::Timeout(
                    "Timed out waiting for challenge validation".to_string(),
                ));
            }
        }

        // Shutdown the HTTP server
        let _ = shutdown_tx.send(());
        let _ = server_handle.await;

        Ok(())
    }

    async fn complete_dns01_challenge(
        &self,
        domain: &str,
        digest: &str,
        order: &mut instant_acme::Order,
        challenge: &instant_acme::Challenge,
    ) -> Result<(), AcmeError> {
        let record_name = format!("_acme-challenge.{}", domain);
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║                    DNS-01 Challenge                            ║");
        println!("╠════════════════════════════════════════════════════════════════╣");
        println!("║ Please add the following TXT record to your DNS:               ║");
        println!("╟────────────────────────────────────────────────────────────────╢");
        println!("║ Name:  {:<56} ║", truncate_str(&record_name, 56));
        println!("║ Type:  TXT                                                     ║");
        println!("║ Value: {:<56} ║", truncate_str(digest, 56));
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        if crate::get_mode() == crate::Mode::NonInteractive {
            return Err(AcmeError::Other(
                "DNS-01 challenge requires interactive mode to confirm DNS record addition"
                    .to_string(),
            ));
        }

        if !ask_for_agreement("Have you added the DNS TXT record? (Press Enter when ready)") {
            return Err(AcmeError::ChallengeFailed(
                "User cancelled DNS-01 challenge".to_string(),
            ));
        }

        // Notify ACME server that challenge is ready
        println!("Notifying ACME server that challenge is ready...");
        order
            .set_challenge_ready(&challenge.url)
            .await
            .map_err(|e| AcmeError::ChallengeFailed(e.to_string()))?;

        // Wait for validation
        println!("Waiting for ACME server to validate DNS record...");
        println!("(This may take a few minutes due to DNS propagation)");

        let mut attempts = 0;
        let max_attempts = 60; // Longer timeout for DNS propagation
        loop {
            sleep(Duration::from_secs(5)).await;

            let auths = order
                .authorizations()
                .await
                .map_err(|e| AcmeError::Authorization(e.to_string()))?;

            let all_valid = auths.iter().all(|a| a.status == AuthorizationStatus::Valid);

            if all_valid {
                println!("[OK] DNS-01 challenge completed successfully");
                break;
            }

            let any_invalid = auths
                .iter()
                .any(|a| a.status == AuthorizationStatus::Invalid);

            if any_invalid {
                return Err(AcmeError::ChallengeFailed(
                    "DNS validation failed. Please verify the TXT record is correct.".to_string(),
                ));
            }

            attempts += 1;
            if attempts >= max_attempts {
                return Err(AcmeError::Timeout(
                    "Timed out waiting for DNS validation. DNS propagation may take longer."
                        .to_string(),
                ));
            }

            if attempts % 6 == 0 {
                println!(
                    "  Still waiting for DNS validation... ({} seconds)",
                    attempts * 5
                );
            }
        }

        Ok(())
    }
}

pub async fn issue_certificate(config: AcmeConfig) -> Result<IssuedCert, AcmeError> {
    let client = AcmeClient::new(&config.email, config.use_staging).await?;
    client
        .issue_cert(&config.domain, config.challenge_method)
        .await
}

pub fn validate_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return false;
    }
    let (local, domain) = (parts[0], parts[1]);
    !local.is_empty() && !domain.is_empty() && domain.contains('.')
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

pub fn validate_domain(domain: &str) -> bool {
    if domain.is_empty() || domain.len() > 253 {
        return false;
    }
    // Must not start or end with a dot or hyphen
    if domain.starts_with('.')
        || domain.ends_with('.')
        || domain.starts_with('-')
        || domain.ends_with('-')
    {
        return false;
    }
    // Must contain at least one dot (TLD required)
    if !domain.contains('.') {
        return false;
    }
    // Each label must be valid
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}
