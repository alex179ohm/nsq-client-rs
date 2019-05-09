use rustls::{
    Certificate, ClientConfig, ClientSession, RootCertStore, ServerCertVerified,
    ServerCertVerifier, TLSError, Session,
};
use std::sync::Arc;
use std::fs;
use std::io::{Read, Write, BufReader};

use webpki_roots::TLS_SERVER_ROOTS;

struct NoCertVerification;

impl ServerCertVerifier for NoCertVerification {
    fn verify_server_cert(
        &self,
        _roots: &RootCertStore,
        _presented_certs: &[Certificate],
        _dns_name: webpki::DNSNameRef<'_>,
        _ocsp_response: &[u8],
    ) -> Result<ServerCertVerified, TLSError> {
        Ok(ServerCertVerified::assertion())
    }
}

#[derive(Debug)]
pub struct TlsSession(pub ClientSession);

impl TlsSession {
    pub fn new(hostname: &str, verify_server_cert: bool) -> TlsSession {
        let dns_name = webpki::DNSNameRef::try_from_ascii_str(hostname).unwrap();
        let mut config = ClientConfig::new();
        config
            .root_store
            .add_server_trust_anchors(&TLS_SERVER_ROOTS);
        if !verify_server_cert {
            config
                .dangerous()
                .set_certificate_verifier(Arc::new(NoCertVerification));
        }
        let mut sess = ClientSession::new(&Arc::new(config), dns_name);
        sess.set_buffer_limit(0);
        //TlsSession(ClientSession::new(&Arc::new(config), dns_name))
        TlsSession(sess)
    }
}

fn load_certs(filename: &str) -> Vec<rustls::Certificate> {
    let certfile = fs::File::open(filename).expect("cannot open certificate file");
    let mut reader = BufReader::new(certfile);
    rustls::internal::pemfile::certs(&mut reader).unwrap()
}

fn load_private_key(filename: &str) -> rustls::PrivateKey {
    let keyfile = fs::File::open(filename).expect("cannot open private key file");
    let mut reader = BufReader::new(keyfile);
    let keys = rustls::internal::pemfile::rsa_private_keys(&mut reader).unwrap();
    match keys.len() {
        0 => { panic!("invalid private key file") },
        1 => {},
        _ => { panic!("multiple keys on private key file") }
    }
    keys[0].clone()
}

#[derive(Debug)]
pub struct TlsConfig {
    ca_file_path: Option<String>,
    check_server_cert: bool,
    server_hostname: String,
}

impl TlsConfig {
    pub fn new(ca_file_path: Option<String>, cert_file_path: String, key_file_path: String) -> TlsConfig {
        TlsConfig {
            ca_file_path,
            cert_file_path,
            key_file_path,
        }
    }
}
