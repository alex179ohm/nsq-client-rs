use rustls::{
    Certificate, ClientConfig, ClientSession, RootCertStore, ServerCertVerified,
    ServerCertVerifier, TLSError, Session,
};
use std::sync::Arc;

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
