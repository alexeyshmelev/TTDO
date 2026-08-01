use bytes::{Buf, Bytes, BytesMut};
use futures::future;
use http::{Request, Response};
use hyper::body::HttpBody;
use log::{info, LevelFilter};
use quiche::h3;
use quiche::h3::NameValue;
use ring::rand::{SecureRandom, SystemRandom};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::aws_lc_rs;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::DigitallySignedStruct;
use std::io::{ErrorKind, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::ops::Deref;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;
use std::{iter, slice};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpStream, UdpSocket};
use tokio_rustls::TlsConnector;
use trusttunnel::authentication::{registry_based::RegistryBasedAuthenticator, Authenticator};
use trusttunnel::core::Core;
use trusttunnel::log_utils;
use trusttunnel::settings::{
    Http1Settings, Http2Settings, ListenProtocolSettings, QuicSettings, Settings, TlsHostInfo,
    TlsHostsSettings,
};
use trusttunnel::shutdown::Shutdown;

pub const MAIN_DOMAIN_NAME: &str = "localhost";
pub const ENDPOINT_IP: Ipv4Addr = Ipv4Addr::LOCALHOST;
pub static NEXT_ENDPOINT_PORT: AtomicU16 = AtomicU16::new(9128);

pub fn set_up_logger() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        log::set_max_level(LevelFilter::Debug);
        log::set_logger(log_utils::make_stdout_logger()).unwrap();
    });
}

pub fn make_endpoint_address() -> SocketAddr {
    (
        ENDPOINT_IP,
        NEXT_ENDPOINT_PORT.fetch_add(1, Ordering::Relaxed),
    )
        .into()
}

pub fn make_cert_key_file() -> File {
    let mut temp_file = tempfile::Builder::new()
        .prefix("vle-")
        .suffix(".pem")
        .tempfile()
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            temp_file.as_file().metadata().unwrap().permissions().mode() & 0o077,
            0
        );
    }
    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let mut params = rcgen::CertificateParams::new(vec![MAIN_DOMAIN_NAME.to_string()]).unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, MAIN_DOMAIN_NAME);
    let cert = params.self_signed(&key_pair).unwrap();
    let cert_key = format!("{}\n{}", cert.pem(), key_pair.serialize_pem());

    temp_file.write_all(cert_key.as_bytes()).unwrap();

    File {
        path: temp_file.path().to_path_buf(),
        _temp_file: temp_file,
    }
}

pub async fn establish_tls_connection(
    server_name: &str,
    peer: &SocketAddr,
    alpn: Option<&[u8]>,
) -> impl AsyncRead + AsyncWrite + Unpin {
    let mut provider = rustls::crypto::aws_lc_rs::default_provider();
    provider.kx_groups = vec![
        aws_lc_rs::kx_group::X25519MLKEM768,
        aws_lc_rs::kx_group::X25519,
        aws_lc_rs::kx_group::SECP256R1,
        aws_lc_rs::kx_group::SECP384R1,
    ];

    let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoopVerifier {}))
        .with_no_client_auth();
    if let Some(alpn) = alpn {
        config.alpn_protocols.push(alpn.to_vec());
    }

    TlsConnector::from(Arc::new(config))
        .connect(
            ServerName::try_from(server_name.to_string()).unwrap(),
            TcpStream::connect(peer).await.unwrap(),
        )
        .await
        .unwrap()
}

pub fn make_stream_of_chunks(
    total_size: usize,
    chunk_size: Option<usize>,
) -> futures::stream::Iter<impl Iterator<Item = &'static [u8]>> {
    const SIZE: usize = 16 * 1024;

    let size = chunk_size.unwrap_or(SIZE);
    assert!(total_size >= size, "{total_size}");
    assert_eq!(total_size % size, 0, "{total_size}");

    static CHUNK: [u8; SIZE] = [0; SIZE];

    futures::stream::iter(iter::repeat(&CHUNK[..size]).take(total_size / size))
}

pub struct File {
    pub path: PathBuf,
    _temp_file: tempfile::NamedTempFile,
}

#[derive(Debug)]
pub struct NoopVerifier;

impl ServerCertVerifier for NoopVerifier {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &CertificateDer<'_>,
        _: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

pub async fn run_endpoint(listen_address: &SocketAddr) {
    let settings = Settings::builder()
        .listen_address(listen_address)
        .unwrap()
        .listen_protocols(ListenProtocolSettings {
            http1: Some(Http1Settings::builder().build()),
            http2: Some(Http2Settings::builder().build()),
            quic: Some(QuicSettings::builder().build()),
        })
        .allow_private_network_connections(true)
        .speedtest_enable(true)
        .ping_enable(true)
        .ping_path("/ping")
        .speedtest_path("/speed")
        .build()
        .unwrap();

    let cert_key_file = make_cert_key_file();
    let cert_key_path = cert_key_file.path.to_str().unwrap();
    let hosts_settings = TlsHostsSettings::builder()
        .main_hosts(vec![TlsHostInfo {
            hostname: MAIN_DOMAIN_NAME.to_string(),
            cert_chain_path: cert_key_path.to_string(),
            private_key_path: cert_key_path.to_string(),
            allowed_sni: vec![],
        }])
        .ping_hosts(vec![TlsHostInfo {
            hostname: format!("ping.{}", MAIN_DOMAIN_NAME),
            cert_chain_path: cert_key_path.to_string(),
            private_key_path: cert_key_path.to_string(),
            allowed_sni: vec![],
        }])
        .speedtest_hosts(vec![TlsHostInfo {
            hostname: format!("speed.{}", MAIN_DOMAIN_NAME),
            cert_chain_path: cert_key_path.to_string(),
            private_key_path: cert_key_path.to_string(),
            allowed_sni: vec![],
        }])
        .reverse_proxy_hosts(vec![TlsHostInfo {
            hostname: format!("hello.{}", MAIN_DOMAIN_NAME),
            cert_chain_path: cert_key_path.to_string(),
            private_key_path: cert_key_path.to_string(),
            allowed_sni: vec![],
        }])
        .build()
        .unwrap();

    run_endpoint_with_settings(settings, hosts_settings).await;
}

pub async fn run_endpoint_with_settings(settings: Settings, hosts_settings: TlsHostsSettings) {
    let shutdown = Shutdown::new();
    let authenticator: Option<Arc<dyn Authenticator>> = if !settings.get_clients().is_empty() {
        Some(Arc::new(RegistryBasedAuthenticator::new(
            settings.get_clients(),
        )))
    } else {
        None
    };

    let endpoint = Core::new(settings, authenticator, hosts_settings, shutdown).unwrap();
    endpoint.listen().await.unwrap();
}

const MAX_QUIC_UDP_PAYLOAD_SIZE: usize = 1350;

pub struct Http3Session {
    socket: UdpSocket,
    quic_conn: quiche::Connection,
    h3_conn: h3::Connection,
    stream_id: Option<u64>,
    is_tunnel: bool, // True for CONNECT requests - don't send FIN
}

impl Http3Session {
    pub async fn connect(peer: &SocketAddr, server_name: &str, alpn: Option<&[u8]>) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();

        let mut scid = [0; quiche::MAX_CONN_ID_LEN];
        SystemRandom::new().fill(&mut scid[..]).unwrap();

        let mut config = quiche::Config::new(quiche::PROTOCOL_VERSION).unwrap();
        config.verify_peer(false);
        config.set_max_idle_timeout(5000);
        config.set_max_recv_udp_payload_size(MAX_QUIC_UDP_PAYLOAD_SIZE);
        config.set_max_send_udp_payload_size(MAX_QUIC_UDP_PAYLOAD_SIZE);
        config.set_initial_max_data(10_000_000);
        config.set_initial_max_stream_data_bidi_local(1_000_000);
        config.set_initial_max_stream_data_bidi_remote(1_000_000);
        config.set_initial_max_stream_data_uni(1_000_000);
        config.set_initial_max_streams_bidi(100);
        config.set_initial_max_streams_uni(100);
        config
            .set_application_protos(
                alpn.as_ref()
                    .map_or(h3::APPLICATION_PROTOCOL, slice::from_ref),
            )
            .unwrap();

        let mut quic_conn = quiche::connect(
            Some(server_name),
            &quiche::ConnectionId::from_ref(&scid),
            socket.local_addr().unwrap(),
            *peer,
            &mut config,
        )
        .unwrap();

        // avoid would block
        tokio::time::sleep(Duration::from_millis(100)).await;
        Self::flush_quic_data(&socket, &mut quic_conn);

        while !quic_conn.is_established() {
            if tokio::time::timeout(quic_conn.timeout().unwrap(), socket.readable())
                .await
                .is_err()
            {
                quic_conn.on_timeout();
            }

            Self::read_out_socket(&socket, &mut quic_conn);
            Self::flush_quic_data(&socket, &mut quic_conn);

            if quic_conn.is_closed() {
                panic!("Closed");
            }
        }

        let h3_conn =
            h3::Connection::with_transport(&mut quic_conn, &h3::Config::new().unwrap()).unwrap();
        Self::flush_quic_data(&socket, &mut quic_conn);

        Self {
            socket,
            quic_conn,
            h3_conn,
            stream_id: Default::default(),
            is_tunnel: false,
        }
    }

    pub async fn exchange(
        &mut self,
        request: Request<hyper::Body>,
    ) -> (http::response::Parts, Bytes) {
        let method = request.method().clone();
        self.send_request(request).await;
        let response = self.recv_response_with_method(&method).await;

        let content_length = (method == http::Method::CONNECT).then_some(0).or_else(|| {
            response
                .headers
                .get(http::header::CONTENT_LENGTH)
                .map(|x| x.to_str().unwrap().parse::<usize>().unwrap())
        });
        let mut content = BytesMut::with_capacity(content_length.unwrap_or_default());
        while content_length.is_none_or(|x| content.len() < x) {
            let mut buffer = [0; 64 * 1024];
            match self.recv(&mut buffer).await {
                0 => break,
                n => content.extend_from_slice(&buffer[..n]),
            }
        }

        (response, content.freeze())
    }

    pub async fn send_request(&mut self, mut request: Request<hyper::Body>) {
        let uri = request.uri();
        let req = iter::once(h3::Header::new(
            b":method",
            request.method().as_str().as_bytes(),
        ))
        .chain(match uri.scheme_str() {
            Some(x) => Box::new(iter::once(h3::Header::new(b":scheme", x.as_bytes())))
                as Box<dyn Iterator<Item = h3::Header>>,
            None => Box::new(iter::empty()) as Box<dyn Iterator<Item = h3::Header>>,
        })
        .chain(iter::once(h3::Header::new(
            b":authority",
            uri.authority().unwrap().as_str().as_bytes(),
        )))
        .chain(match uri.path_and_query() {
            Some(x) => Box::new(iter::once(h3::Header::new(b":path", x.as_str().as_bytes())))
                as Box<dyn Iterator<Item = h3::Header>>,
            None => Box::new(iter::empty()) as Box<dyn Iterator<Item = h3::Header>>,
        })
        .chain(
            request
                .headers()
                .iter()
                .map(|(n, v)| h3::Header::new(n.as_str().as_bytes(), v.as_bytes())),
        )
        .collect::<Vec<_>>();

        // Always send request with fin=false, we'll send FIN separately after body
        self.stream_id = Some(
            self.h3_conn
                .send_request(&mut self.quic_conn, &req, false)
                .unwrap(),
        );
        Self::flush_quic_data(&self.socket, &mut self.quic_conn);

        while let Some(mut chunk) = request.body_mut().data().await.map(Result::unwrap) {
            while !chunk.is_empty() {
                let stream_id = self.stream_id();
                match self
                    .h3_conn
                    .send_body(&mut self.quic_conn, stream_id, &chunk, false)
                {
                    Ok(n) => chunk.advance(n),
                    Err(h3::Error::Done) => {
                        Self::flush_quic_data(&self.socket, &mut self.quic_conn);
                        if tokio::time::timeout(
                            self.quic_conn.timeout().unwrap(),
                            self.socket.readable(),
                        )
                        .await
                        .is_err()
                        {
                            self.quic_conn.on_timeout();
                        }
                    }
                    Err(e) => panic!("{}", e),
                }

                Self::read_out_socket(&self.socket, &mut self.quic_conn);
                Self::flush_quic_data(&self.socket, &mut self.quic_conn);
            }
        }

        Self::flush_quic_data(&self.socket, &mut self.quic_conn);
    }

    pub async fn recv_response(&mut self) -> http::response::Parts {
        self.recv_response_with_method(&http::Method::GET).await
    }

    async fn recv_response_with_method(&mut self, method: &http::Method) -> http::response::Parts {
        Self::read_out_socket(&self.socket, &mut self.quic_conn);
        Self::flush_quic_data(&self.socket, &mut self.quic_conn);

        loop {
            match self.poll().await {
                h3::Event::Headers { list, .. } => {
                    let mut response = Response::builder().version(http::Version::HTTP_3);
                    for h in list {
                        match h.name() {
                            b":status" => response = response.status(h.value()),
                            _ => response = response.header(h.name(), h.value()),
                        }
                    }

                    let response = response.body(()).unwrap().into_parts().0;
                    info!("Received response: {:?}", response);

                    // Track if this is a CONNECT tunnel - don't send FIN for tunnels
                    if method == http::Method::CONNECT {
                        self.is_tunnel = true;
                    }

                    if !self.is_tunnel {
                        loop {
                            let stream_id = self.stream_id();
                            match self
                                .h3_conn
                                .send_body(&mut self.quic_conn, stream_id, &[], true)
                            {
                                Ok(_) => break,
                                Err(h3::Error::Done) => {
                                    Self::flush_quic_data(&self.socket, &mut self.quic_conn);
                                    if tokio::time::timeout(
                                        self.quic_conn.timeout().unwrap(),
                                        self.socket.readable(),
                                    )
                                    .await
                                    .is_err()
                                    {
                                        self.quic_conn.on_timeout();
                                    }
                                    Self::read_out_socket(&self.socket, &mut self.quic_conn);
                                }
                                // If stream/connection already closed, that's fine
                                Err(
                                    h3::Error::TransportError(_)
                                    | h3::Error::StreamBlocked
                                    | h3::Error::IdError,
                                ) => {
                                    break;
                                }
                                Err(e) => panic!("Failed to finish stream: {}", e),
                            }
                        }
                        Self::flush_quic_data(&self.socket, &mut self.quic_conn);
                    }

                    return response;
                }
                h3::Event::Finished => {
                    // Client-side FIN received (normal for requests without body)
                    // Continue polling for response headers
                    continue;
                }
                x => unreachable!("{:?}", x),
            }
        }
    }

    fn read_out_socket(socket: &UdpSocket, quic_conn: &mut quiche::Connection) {
        let mut buffer = [0; MAX_QUIC_UDP_PAYLOAD_SIZE];
        loop {
            match socket.try_recv_from(&mut buffer) {
                Ok((n, peer)) => {
                    let recv_info = quiche::RecvInfo {
                        from: peer,
                        to: socket.local_addr().unwrap(),
                    };
                    let x = quic_conn.recv(&mut buffer[..n], recv_info).unwrap();
                    assert_eq!(n, x);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => panic!("{}", e),
            }
        }
    }

    pub async fn send(
        &mut self,
        mut stream: impl futures::stream::Stream<Item = impl Deref<Target = [u8]>> + Unpin,
    ) {
        while let Some(mut chunk) =
            futures::future::poll_fn(|cx| Pin::new(&mut stream).poll_next(cx))
                .await
                .as_deref()
        {
            while !chunk.is_empty() {
                let stream_id = self.stream_id();
                match self
                    .h3_conn
                    .send_body(&mut self.quic_conn, stream_id, chunk, false)
                {
                    Ok(n) => chunk = &chunk[n..],
                    Err(h3::Error::Done) => {
                        Self::flush_quic_data(&self.socket, &mut self.quic_conn);
                        if tokio::time::timeout(
                            self.quic_conn.timeout().unwrap(),
                            self.socket.readable(),
                        )
                        .await
                        .is_err()
                        {
                            self.quic_conn.on_timeout();
                        }
                    }
                    Err(e) => panic!("{}", e),
                }

                Self::read_out_socket(&self.socket, &mut self.quic_conn);
                Self::flush_quic_data(&self.socket, &mut self.quic_conn);
            }
        }

        // Don't send FIN for CONNECT tunnels - they remain bidirectional
        if !self.is_tunnel {
            loop {
                let stream_id = self.stream_id();
                match self
                    .h3_conn
                    .send_body(&mut self.quic_conn, stream_id, &[], true)
                {
                    Ok(_) => break,
                    Err(h3::Error::Done) => {
                        Self::flush_quic_data(&self.socket, &mut self.quic_conn);
                        if tokio::time::timeout(
                            self.quic_conn.timeout().unwrap(),
                            self.socket.readable(),
                        )
                        .await
                        .is_err()
                        {
                            self.quic_conn.on_timeout();
                        }
                        Self::read_out_socket(&self.socket, &mut self.quic_conn);
                    }
                    // If stream/connection already closed
                    Err(
                        h3::Error::TransportError(_)
                        | h3::Error::StreamBlocked
                        | h3::Error::IdError,
                    ) => {
                        break;
                    }
                    Err(e) => panic!("Failed to finish stream: {}", e),
                }
            }
        }

        Self::flush_quic_data(&self.socket, &mut self.quic_conn);
    }

    pub async fn recv(&mut self, buf: &mut [u8]) -> usize {
        let ret = loop {
            Self::read_out_socket(&self.socket, &mut self.quic_conn);

            let stream_id = self.stream_id();
            match self.h3_conn.recv_body(&mut self.quic_conn, stream_id, buf) {
                Ok(n) => break n,
                Err(h3::Error::Done) => (),
                Err(e) => panic!("{}", e),
            }

            Self::flush_quic_data(&self.socket, &mut self.quic_conn);

            if tokio::time::timeout(self.quic_conn.timeout().unwrap(), self.socket.readable())
                .await
                .is_err()
            {
                self.quic_conn.on_timeout();
            }

            Self::read_out_socket(&self.socket, &mut self.quic_conn);

            match self.poll().await {
                h3::Event::Data => (),
                h3::Event::Finished | h3::Event::Reset(_) => break 0,
                x => unreachable!("{:?}", x),
            }
        };

        Self::flush_quic_data(&self.socket, &mut self.quic_conn);

        ret
    }

    fn flush_quic_data(socket: &UdpSocket, quic_conn: &mut quiche::Connection) {
        let mut buffer = [0; MAX_QUIC_UDP_PAYLOAD_SIZE];
        loop {
            match quic_conn.send(&mut buffer) {
                Ok((n, send_info)) => match socket.try_send_to(&buffer[..n], send_info.to) {
                    Ok(_) => (),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(e) => panic!("{}", e),
                },
                Err(quiche::Error::Done) => break,
                Err(e) => panic!("{}", e),
            }
        }
    }

    async fn poll(&mut self) -> h3::Event {
        Self::read_out_socket(&self.socket, &mut self.quic_conn);

        let ret = loop {
            match self.h3_conn.poll(&mut self.quic_conn) {
                Ok((stream_id, event)) => {
                    assert_eq!(stream_id, self.stream_id.unwrap());
                    break event;
                }
                Err(h3::Error::Done) => (),
                Err(e) => panic!("{}", e),
            }

            Self::flush_quic_data(&self.socket, &mut self.quic_conn);
            if tokio::time::timeout(self.quic_conn.timeout().unwrap(), self.socket.readable())
                .await
                .is_err()
            {
                self.quic_conn.on_timeout();
            }
            Self::read_out_socket(&self.socket, &mut self.quic_conn);

            if self.quic_conn.is_closed() {
                panic!("Closed");
            }
        };

        Self::flush_quic_data(&self.socket, &mut self.quic_conn);

        ret
    }

    fn stream_id(&self) -> u64 {
        self.stream_id.unwrap()
    }
}

pub async fn do_get_request<IO>(
    io: IO,
    version: http::Version,
    url: &str,
    extra_headers: &[(&str, &str)],
) -> (http::response::Parts, Bytes)
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut request, conn) = hyper::client::conn::Builder::new()
        .http2_only(version == http::Version::HTTP_2)
        .handshake(io)
        .await
        .unwrap();

    let mut request_builder = hyper::Request::get(url).version(version);
    for (n, v) in extra_headers {
        request_builder = request_builder.header(*n, *v);
    }

    let exchange = async {
        let response = request
            .send_request(request_builder.body(hyper::Body::empty()).unwrap())
            .await
            .unwrap();
        info!("Received response: {:?}", response);

        let (parts, body) = response.into_parts();
        (parts, hyper::body::to_bytes(body).await.unwrap())
    };

    futures::pin_mut!(exchange);
    match future::select(conn, exchange).await {
        future::Either::Left((r, exchange)) => {
            info!("HTTP connection closed with result: {:?}", r);
            exchange.await
        }
        future::Either::Right((response, _)) => response,
    }
}

pub async fn do_post_request<IO>(
    io: IO,
    version: http::Version,
    url: &str,
    content_length: usize,
) -> Response<hyper::Body>
where
    IO: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut request, conn) = hyper::client::conn::Builder::new()
        .http2_only(version == http::Version::HTTP_2)
        .handshake(io)
        .await
        .unwrap();

    let exchange = async {
        let req = hyper::Request::post(url)
            .version(version)
            .body(hyper::Body::from(vec![0; content_length]))
            .unwrap();

        let response = request.send_request(req).await.unwrap();

        info!("Received response: {:?}", response);
        response
    };

    futures::pin_mut!(exchange);
    match future::select(conn, exchange).await {
        future::Either::Left((r, exchange)) => {
            info!("HTTP connection closed with result: {:?}", r);
            exchange.await
        }
        future::Either::Right((response, _)) => response,
    }
}
