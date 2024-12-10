use std::borrow::BorrowMut;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;

use quinn::{ClientConfig, Endpoint, ServerConfig};
use tokio::runtime;
use tokio_stream::StreamExt;

use crate::executor::{Executor, LocalExec};
use crate::util::Shutdown;
use crate::Result;

pub fn spawn(
    address: SocketAddr,
    exec: LocalExec<(SocketAddr, Vec<u8>), Vec<u8>>,
    runtime: runtime::Handle,
    mut shutdown: Shutdown,
) {
    runtime.clone().spawn(async move {
        let (endpoint, _cert) = make_server_endpoint(address).unwrap();

        while let Some(conn) = endpoint.accept().await {
            let exec = exec.clone();
            info!("connection incoming");
            let runtime = runtime.clone();
            runtime.clone().spawn(async move {
                if let Err(e) = handle_connection(exec.clone(), conn, runtime.clone()).await {
                    error!("connection failed: {reason}", reason = e.to_string())
                }
            });
        }
    });
}

async fn handle_connection(
    exec: LocalExec<(SocketAddr, Vec<u8>), Vec<u8>>,
    conn: quinn::Connecting,
    runtime: runtime::Handle,
) -> Result<()> {
    trace!("handling connection");
    let mut connection = conn.await.unwrap();
    // let span = info_span!(
    //     "connection",
    //     remote = %connection.remote_address(),
    //     protocol = %connection
    //         .handshake_data()
    //         .unwrap()
    //         .downcast::<quinn::crypto::rustls::HandshakeData>().unwrap()
    //         .protocol
    //         .map_or_else(|| "<none>".into(), |x| String::from_utf8_lossy(&x).into_owned())
    // );
    async {
        trace!("connection established");

        let remote_addr = connection.remote_address();

        // Each stream initiated by the client constitutes a new request.
        loop {
            let stream = connection.accept_bi().await;
            let (mut send, mut recv) = match stream {
                Err(quinn::ConnectionError::ApplicationClosed { .. }) => {
                    warn!("connection closed");
                    panic!();
                }
                Err(e) => {
                    error!("{:?}", e);
                    panic!();
                }
                Ok(s) => s,
            };

            // println!("got streams, start reading");

            let exec = exec.clone();
            runtime.clone().spawn(async move {
                let bytes = recv.read_to_end(100000000).await.unwrap();
                // println!("read: {:?}", bytes);

                let resp = exec.execute((remote_addr, bytes)).await.unwrap();
                // println!("resp: {:?}", resp);

                send.write_all(&resp).await.unwrap();
                send.finish().await.unwrap();

                // let fut = handle_request(root.clone(), stream);
                // runtime.spawn(
                //     async move {
                //     },
                // );
            });
        }
    }
    // .instrument(span)
    .await;
    Ok(())
}

/// Constructs a QUIC endpoint configured for use as client-only.
///
/// Includes a flag for enabling native root certificates and optional list
/// of custom server certificates.
pub fn make_client_endpoint(
    bind_addr: SocketAddr,
    native_roots: bool,
    server_certs: Option<Vec<Vec<u8>>>,
) -> std::result::Result<Endpoint, Box<dyn std::error::Error>> {
    let client_cfg = configure_client(native_roots, server_certs)?;
    let mut endpoint = Endpoint::client(bind_addr)?;
    endpoint.set_default_client_config(client_cfg);
    Ok(endpoint)
}

/// Constructs a QUIC endpoint configured for use as client-only, bypassing
/// TLS certificate requirements.
pub fn make_client_endpoint_insecure(
    bind_addr: SocketAddr,
) -> std::result::Result<Endpoint, Box<dyn std::error::Error>> {
    let client_cfg = configure_client_insecure();
    let mut endpoint = Endpoint::client(bind_addr)?;
    endpoint.set_default_client_config(client_cfg);
    Ok(endpoint)
}

/// Constructs a QUIC endpoint configured to listen for incoming connections
/// on a certain address and port.
///
/// Returns a stream of incoming QUIC connections and server certificate
/// serialized into DER format
#[allow(unused)]
pub fn make_server_endpoint(
    bind_addr: SocketAddr,
) -> std::result::Result<(Endpoint, Vec<u8>), Box<dyn std::error::Error>> {
    let (server_config, server_cert) = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok((endpoint, server_cert))
}

/// Builds default quinn client config and trusts given certificates.
fn configure_client(
    native_roots: bool,
    server_certs: Option<Vec<Vec<u8>>>,
) -> std::result::Result<ClientConfig, Box<dyn std::error::Error>> {
    let mut roots = rustls::RootCertStore::empty();
    if native_roots {
        match rustls_native_certs::load_native_certs() {
            Ok(certs) => {
                for cert in certs {
                    if let Err(e) = roots.add(&rustls::Certificate(cert.0)) {
                        warn!("failed to parse trust anchor: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("couldn't load any default trust roots: {}", e);
            }
        };
    }
    if let Some(server_certs) = server_certs {
        for cert in server_certs {
            roots.add(&rustls::Certificate(cert))?;
        }
    }

    Ok(ClientConfig::with_root_certificates(roots))
}

/// Builds quinn client config that will skip server verification
/// and client auth.
fn configure_client_insecure() -> ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(SkipServerVerification::new())
        .with_no_client_auth();

    ClientConfig::new(Arc::new(crypto))
}

/// Returns default server configuration along with its certificate.
fn configure_server() -> std::result::Result<(ServerConfig, Vec<u8>), Box<dyn std::error::Error>> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_der = cert.serialize_der().unwrap();
    let priv_key = cert.serialize_private_key_der();
    let priv_key = rustls::PrivateKey(priv_key);
    let cert_chain = vec![rustls::Certificate(cert_der.clone())];

    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key)?;
    Arc::get_mut(&mut server_config.transport)
        .unwrap()
        .max_concurrent_uni_streams(0_u8.into());

    Ok((server_config, cert_der))
}

#[allow(unused)]
pub const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-29"];

// Implementation of `ServerCertVerifier` that verifies everything as trustworthy.
struct SkipServerVerification;

impl SkipServerVerification {
    fn new() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> std::result::Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}
