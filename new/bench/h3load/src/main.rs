use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use bytes::Buf;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

#[derive(Debug)]
struct Blind;

impl ServerCertVerifier for Blind {

    fn verify_server_cert ( &self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime ) -> Result<ServerCertVerified, rustls::Error> {

        Ok(ServerCertVerified::assertion())

    }

    fn verify_tls12_signature ( &self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct ) -> Result<HandshakeSignatureValid, rustls::Error> {

        Ok(HandshakeSignatureValid::assertion())

    }

    fn verify_tls13_signature ( &self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct ) -> Result<HandshakeSignatureValid, rustls::Error> {

        Ok(HandshakeSignatureValid::assertion())

    }

    fn supported_verify_schemes ( &self ) -> Vec<SignatureScheme> {

        rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms.supported_schemes()

    }

}

struct Tally {
    done   : AtomicU64,
    failed : AtomicU64,
    stop   : AtomicBool,
    shown  : AtomicBool,
}

type Fault = Box<dyn std::error::Error + Send + Sync>;

async fn worker ( mut sender: h3::client::SendRequest<h3_quinn::OpenStreams, bytes::Bytes>, uri: http::Uri, tally: Arc<Tally> ) -> Vec<u32> {

    let mut samples = Vec::with_capacity(65_536);

    while !tally.stop.load(Ordering::Relaxed) {

        let started = Instant::now();

        let outcome: Result<http::StatusCode, Fault> = async {

            let request = http::Request::builder().method("GET").uri(uri.clone()).body(())?;
            let mut stream = sender.send_request(request).await?;

            stream.finish().await?;

            let response = stream.recv_response().await?;

            while let Some(mut chunk) = stream.recv_data().await? { let size = chunk.remaining(); chunk.advance(size); }

            Ok(response.status())

        }.await;

        match outcome {
            Ok(status) if status.is_success() => { tally.done.fetch_add(1, Ordering::Relaxed); samples.push(started.elapsed().as_micros().min(u128::from(u32::MAX)) as u32); }
            other => {

                if !tally.shown.swap(true, Ordering::Relaxed) { eprintln!("first failure: {other:?}"); }

                tally.failed.fetch_add(1, Ordering::Relaxed);

                if other.is_err() { tokio::time::sleep(Duration::from_millis(1)).await; }

            }
        }

    }

    samples

}

#[tokio::main(flavor = "multi_thread")]
async fn main () -> Result<(), Fault> {

    let arguments: Vec<String> = std::env::args().collect();
    let number = |flag: &str, fallback: u64| arguments.iter().position(|argument| argument == flag).and_then(|at| arguments.get(at + 1)).and_then(|value| value.parse().ok()).unwrap_or(fallback);
    let uri: http::Uri = arguments.last().filter(|_| arguments.len() > 1).ok_or("usage: h3load [-c connections] [-m streams] [-d seconds] https://host:port/path")?.parse()?;
    let ( connections, streams, seconds ) = ( number("-c", 16), number("-m", 10), number("-d", 10) );
    let address: SocketAddr = format!("{}:{}", uri.host().ok_or("the url needs a host")?, uri.port_u16().unwrap_or(443)).parse()?;

    let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider())).with_safe_default_protocol_versions()?.dangerous().with_custom_certificate_verifier(Arc::new(Blind)).with_no_client_auth();

    tls.alpn_protocols = vec![b"h3".to_vec()];

    let config = quinn::ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(tls)?));
    let tally = Arc::new(Tally { done: AtomicU64::new(0), failed: AtomicU64::new(0), stop: AtomicBool::new(false), shown: AtomicBool::new(false) });
    let mut workers = Vec::new();

    for _ in 0..connections {

        let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;

        endpoint.set_default_client_config(config.clone());

        let connection = endpoint.connect(address, uri.host().unwrap_or("localhost"))?.await?;
        let ( mut driver, sender ) = h3::client::new(h3_quinn::Connection::new(connection)).await?;

        tokio::spawn(async move { let _ = std::future::poll_fn(|cx| driver.poll_close(cx)).await; });

        for _ in 0..streams { workers.push(tokio::spawn(worker(sender.clone(), uri.clone(), tally.clone()))); }

    }

    let started = Instant::now();

    tokio::time::sleep(Duration::from_secs(seconds)).await;
    tally.stop.store(true, Ordering::Relaxed);

    let elapsed = started.elapsed().as_secs_f64();
    let mut samples: Vec<u32> = Vec::new();

    for worker in workers { if let Ok(Ok(mut part)) = tokio::time::timeout(Duration::from_secs(5), worker).await { samples.append(&mut part); } }

    samples.sort_unstable();

    let at = |quantile: f64| samples.get(((samples.len() as f64 * quantile) as usize).min(samples.len().saturating_sub(1))).map_or(0.0, |micros| f64::from(*micros) / 1_000.0);
    let done = tally.done.load(Ordering::Relaxed);

    println!("requests: {done}");
    println!("errors: {}", tally.failed.load(Ordering::Relaxed));
    println!("Requests/sec: {:.2}", done as f64 / elapsed);
    println!("p50_ms: {:.3}", at(0.50));
    println!("p90_ms: {:.3}", at(0.90));
    println!("p99_ms: {:.3}", at(0.99));

    Ok(())

}
