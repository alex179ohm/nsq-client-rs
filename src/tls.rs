use crate::config::VerifyServerCert;
use rustls::{
    Certificate, ClientConfig, ClientSession, RootCertStore, ServerCertVerified,
    ServerCertVerifier, TLSError,
};
use std::fs;
use std::io::BufReader;
use std::sync::Arc;

use log::{debug, warn};
use untrusted;
use webpki;
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

type SignatureAlgorithms = &'static [&'static webpki::SignatureAlgorithm];

static SUPPORTED_SIG_ALGS: SignatureAlgorithms = &[
    &webpki::ECDSA_P256_SHA256,
    &webpki::ECDSA_P256_SHA384,
    &webpki::ECDSA_P384_SHA256,
    &webpki::ECDSA_P384_SHA384,
    &webpki::RSA_PSS_2048_8192_SHA256_LEGACY_KEY,
    &webpki::RSA_PSS_2048_8192_SHA384_LEGACY_KEY,
    &webpki::RSA_PSS_2048_8192_SHA512_LEGACY_KEY,
    &webpki::RSA_PKCS1_2048_8192_SHA256,
    &webpki::RSA_PKCS1_2048_8192_SHA384,
    &webpki::RSA_PKCS1_2048_8192_SHA512,
    &webpki::RSA_PKCS1_3072_8192_SHA384,
];

struct PrivateVerification {
    pub time: fn() -> Result<webpki::Time, TLSError>,
}

impl ServerCertVerifier for PrivateVerification {
    fn verify_server_cert(
        &self,
        roots: &RootCertStore,
        presented_certs: &[Certificate],
        dns_name: webpki::DNSNameRef,
        _ocsp_response: &[u8],
    ) -> Result<ServerCertVerified, TLSError> {
        let (cert, chain, trustroots) = prepare(roots, presented_certs)?;
        //debug!("cert: {:?}", cert);
        debug!("chain: {:?}", chain);
        debug!("roots: {:?}", trustroots);
        let now = (self.time)()?;
        let res_cert = cert.verify_is_valid_tls_server_cert(
            SUPPORTED_SIG_ALGS,
            &webpki::TLSServerTrustAnchors(&trustroots),
            &chain,
            now,
        );
        if res_cert.is_err() {
            warn!("cert: {:?}", res_cert);
        } else {
            debug!("cert: {:?}", res_cert);
        }
        let res_dns_name = cert.verify_is_valid_for_dns_name(dns_name);
        if res_dns_name.is_err() {
            warn!("dns_name: {:?}", res_dns_name);
        }
        Ok(ServerCertVerified::assertion())
    }
}

impl PrivateVerification {
    fn new() -> PrivateVerification {
        PrivateVerification { time: try_now }
    }
}

fn try_now() -> Result<webpki::Time, TLSError> {
    webpki::Time::try_from(std::time::SystemTime::now())
        .map_err(|_| TLSError::FailedToGetCurrentTime)
}

fn prepare<'a, 'b>(
    roots: &'b RootCertStore,
    presented_certs: &'a [Certificate],
) -> Result<
    (
        webpki::EndEntityCert<'a>,
        Vec<untrusted::Input<'a>>,
        Vec<webpki::TrustAnchor<'b>>,
    ),
    TLSError,
> {
    if presented_certs.is_empty() {
        return Err(TLSError::NoCertificatesPresented);
    }

    // EE cert must appear first.
    let cert_der = untrusted::Input::from(&presented_certs[0].0);
    let cert = webpki::EndEntityCert::from(cert_der).map_err(TLSError::WebPKIError)?;

    let chain: Vec<untrusted::Input> = presented_certs
        .iter()
        .skip(1)
        .map(|cert| untrusted::Input::from(&cert.0))
        .collect();

    let trustroots: Vec<webpki::TrustAnchor> =
        roots.roots.iter().map(|x| x.to_trust_anchor()).collect();

    Ok((cert, chain, trustroots))
}

#[derive(Debug)]
pub struct TlsSession(pub ClientSession);

impl TlsSession {
    pub fn new<S: Into<String> + Clone>(
        hostname: &str,
        verify_server_cert: VerifyServerCert<S>,
    ) -> TlsSession {
        let dns_name = webpki::DNSNameRef::try_from_ascii_str(hostname).unwrap();
        let mut config = ClientConfig::new();
        match verify_server_cert {
            VerifyServerCert::None => {
                config
                    .dangerous()
                    .set_certificate_verifier(Arc::new(NoCertVerification));
            }
            VerifyServerCert::PrivateCA(s) => {
                let private_ca_root_path = s.into();
                let certfile = fs::File::open(&private_ca_root_path).expect("cannot open CA file");
                let mut reader = BufReader::new(certfile);
                config.root_store.add_pem_file(&mut reader).unwrap();
                config
                    .dangerous()
                    .set_certificate_verifier(Arc::new(PrivateVerification::new()));
            }
            VerifyServerCert::PublicCA => {
                config
                    .root_store
                    .add_server_trust_anchors(&TLS_SERVER_ROOTS);
            }
        };
        let sess = ClientSession::new(&Arc::new(config), dns_name);
        TlsSession(sess)
    }
}
