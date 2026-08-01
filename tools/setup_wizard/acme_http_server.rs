use crate::acme::AcmeError;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const ACME_CHALLENGE_PATH_PREFIX: &str = "/.well-known/acme-challenge/";
const HTTP_PORT: u16 = 80;

struct ChallengeData {
    token: String,
    key_authorization: String,
}

async fn handle_request(
    req: Request<hyper::body::Incoming>,
    challenge_data: Arc<ChallengeData>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path();

    if let Some(request_token) = path.strip_prefix(ACME_CHALLENGE_PATH_PREFIX) {
        if request_token == challenge_data.token {
            println!(
                "  [OK] Serving ACME challenge response for token: {}",
                request_token
            );
            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/plain")
                .body(Full::new(Bytes::from(
                    challenge_data.key_authorization.clone(),
                )))
                .unwrap());
        } else {
            println!(
                "  [ERROR] Token mismatch: expected {}, got {}",
                challenge_data.token, request_token
            );
        }
    }

    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("Not Found")))
        .unwrap())
}

pub async fn run_http01_challenge_server(
    token: String,
    key_authorization: String,
) -> Result<(oneshot::Sender<()>, JoinHandle<()>), AcmeError> {
    let addr = SocketAddr::from(([0, 0, 0, 0], HTTP_PORT));

    // Check if port is available
    let listener = TcpListener::bind(addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            AcmeError::PermissionDenied(HTTP_PORT)
        } else {
            AcmeError::PortInUse(HTTP_PORT)
        }
    })?;

    println!(
        "  HTTP server listening on port {} for ACME challenge",
        HTTP_PORT
    );

    let challenge_data = Arc::new(ChallengeData {
        token,
        key_authorization,
    });

    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let io = TokioIo::new(stream);
                            let challenge_data = challenge_data.clone();

                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    let challenge_data = challenge_data.clone();
                                    async move { handle_request(req, challenge_data).await }
                                });

                                if let Err(e) = http1::Builder::new()
                                    .serve_connection(io, service)
                                    .await
                                {
                                    eprintln!("  HTTP server error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("  Failed to accept connection: {}", e);
                        }
                    }
                }
                _ = &mut shutdown_rx => {
                    println!("  HTTP server shutting down");
                    break;
                }
            }
        }
    });

    Ok((shutdown_tx, handle))
}
