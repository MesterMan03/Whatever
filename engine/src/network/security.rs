use super::settings::NetworkSettings;
use anyhow::{Context, Result, ensure};
use rustls::{
    DigitallySignedStruct, RootCertStore, SignatureScheme,
    client::{
        WebPkiServerVerifier,
        danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    },
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub const ALPN: &[u8] = b"whatever/1";

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Trust {
    #[default]
    Strict,
    Tofu,
}

fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().context("identity directory")?)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Publish a complete immutable file without ever replacing an existing identity.
        fs::hard_link(&temporary, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(temporary);
    result
}

#[derive(Serialize, Deserialize)]
struct Identity {
    certificate: Vec<u8>,
    key: Vec<u8>,
}
pub fn server_config(settings: &NetworkSettings) -> Result<quinn::ServerConfig> {
    let (certs, key): (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>) =
        match (&settings.certificate, &settings.private_key) {
            (Some(cert), Some(key)) => {
                let certificates = rustls_pemfile::certs(&mut &fs::read(cert)?[..])
                    .collect::<std::io::Result<Vec<_>>>()?;
                let key = rustls_pemfile::private_key(&mut &fs::read(key)?[..])?
                    .context("no private key in TLS key file")?;
                (certificates, key)
            }
            (None, None) => {
                let path = settings.directory.join("server-identity.json");
                if !path.exists() {
                    let generated = rcgen::generate_simple_self_signed(vec![
                        "localhost".into(),
                        "127.0.0.1".into(),
                        "::1".into(),
                    ])?;
                    let identity = Identity {
                        certificate: generated.cert.der().to_vec(),
                        key: generated.signing_key.serialize_der(),
                    };
                    // A competing engine may have created the identity first; always load the winner.
                    if let Err(error) = private_write(&path, &serde_json::to_vec(&identity)?)
                        && !path.exists()
                    {
                        return Err(error);
                    }
                }
                let identity: Identity = serde_json::from_slice(&fs::read(path)?)?;
                (
                    vec![CertificateDer::from(identity.certificate)],
                    PrivatePkcs8KeyDer::from(identity.key).into(),
                )
            }
            _ => anyhow::bail!("--tls-cert and --tls-key must be supplied together"),
        };
    let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .with_no_client_auth()
    .with_single_cert(certs, key)?;
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let mut config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls)?,
    ));
    config.transport_config(transport_config());
    Ok(config)
}
pub fn transport_config() -> Arc<quinn::TransportConfig> {
    let mut config = quinn::TransportConfig::default();
    config.max_concurrent_bidi_streams(2u8.into());
    config.max_concurrent_uni_streams(0u8.into());
    config.max_idle_timeout(Some(
        std::time::Duration::from_secs(30)
            .try_into()
            .expect("valid idle timeout"),
    ));
    config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
    config.receive_window((4u32 * 1_048_576).into());
    config.stream_receive_window((2u32 * 1_048_576).into());
    config.send_window(4 * 1_048_576);
    config.datagram_receive_buffer_size(Some(256 * 1024));
    config.datagram_send_buffer_size(256 * 1024);
    Arc::new(config)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Record {
    Pki,
    Pinned { key: String },
}
#[derive(Debug)]
pub struct TrustStore {
    directory: PathBuf,
    records: Mutex<HashMap<String, Record>>,
}
impl TrustStore {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            records: Mutex::new(HashMap::new()),
        }
    }
    fn path(&self, address: &str) -> PathBuf {
        self.directory
            .join(format!("{}.json", digest(address.as_bytes())))
    }
    fn read(&self, address: &str) -> Result<Option<Record>> {
        let path = self.path(address);
        match fs::read(path) {
            Ok(bytes) => Ok(Some(
                serde_json::from_slice(&bytes).context("invalid saved server identity")?,
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
    fn save(&self, address: &str, record: &Record) -> Result<()> {
        let mut records = self
            .records
            .lock()
            .map_err(|_| anyhow::anyhow!("trust store lock poisoned"))?;
        let existing = self
            .read(address)?
            .or_else(|| records.get(address).cloned());
        if let Some(existing) = existing {
            ensure!(
                &existing == record,
                "server identity changed during connection"
            );
        } else {
            let path = self.path(address);
            if let Err(error) = private_write(&path, &serde_json::to_vec(record)?) {
                // Also protects first use against two engine processes racing to pin different keys.
                ensure!(
                    self.read(address)?.as_ref() == Some(record),
                    "cannot save server identity: {error}"
                );
            }
        }
        records.insert(address.into(), record.clone());
        Ok(())
    }
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug)]
pub struct Verifier {
    policy: Trust,
    web: Option<Arc<WebPkiServerVerifier>>,
    store: Arc<TrustStore>,
    address: String,
    existing: Option<Record>,
    observed: Mutex<Option<Record>>,
}
impl Verifier {
    pub fn new(
        settings: &NetworkSettings,
        store: Arc<TrustStore>,
        address: String,
        policy: Trust,
    ) -> Result<Arc<Self>> {
        let mut roots = RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().certs {
            let _ = roots.add(cert);
        }
        if let Some(path) = &settings.authority {
            for cert in rustls_pemfile::certs(&mut &fs::read(path)?[..]) {
                roots.add(cert?)?;
            }
        }
        let web = if roots.is_empty() {
            None
        } else {
            Some(
                WebPkiServerVerifier::builder_with_provider(
                    Arc::new(roots),
                    Arc::new(rustls::crypto::ring::default_provider()),
                )
                .build()?,
            )
        };
        ensure!(
            policy != Trust::Strict || web.is_some(),
            "no trusted certificate authorities available"
        );
        let existing = if policy == Trust::Tofu {
            store.read(&address)?
        } else {
            None
        };
        Ok(Arc::new(Self {
            policy,
            web,
            store,
            address,
            existing,
            observed: Mutex::new(None),
        }))
    }
    /// Called after TLS proves possession of the private key AND the application handshake succeeds.
    pub fn commit(&self) -> Result<()> {
        if self.policy == Trust::Tofu {
            let record = self
                .observed
                .lock()
                .map_err(|_| anyhow::anyhow!("identity lock poisoned"))?
                .clone()
                .context("missing TLS identity")?;
            self.store.save(&self.address, &record)?;
        }
        Ok(())
    }
}
impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let parsed = rustls::server::ParsedCertificate::try_from(cert)?;
        let record = Record::Pinned {
            key: digest(parsed.subject_public_key_info().as_ref()),
        };
        let web = self
            .web
            .as_ref()
            .map(|verifier| verifier.verify_server_cert(cert, intermediates, name, ocsp, now));
        let accepted = if self.policy == Trust::Strict || self.existing == Some(Record::Pki) {
            web.ok_or_else(|| {
                rustls::Error::General("no trusted certificate authorities".into())
            })??;
            Record::Pki
        } else if let Some(existing) = &self.existing {
            if existing != &record {
                return Err(rustls::Error::General(
                    "server identity changed; saved TOFU key does not match".into(),
                ));
            }
            record
        } else if matches!(web, Some(Ok(_))) {
            Record::Pki
        } else {
            record
        };
        *self
            .observed
            .lock()
            .map_err(|_| rustls::Error::General("identity lock poisoned".into()))? = Some(accepted);
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
pub fn client_config(verifier: Arc<Verifier>) -> Result<quinn::ClientConfig> {
    let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_protocol_versions(&[&rustls::version::TLS13])?
    .dangerous()
    .with_custom_certificate_verifier(verifier)
    .with_no_client_auth();
    tls.alpn_protocols = vec![ALPN.to_vec()];
    let mut config = quinn::ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(tls)?,
    ));
    config.transport_config(transport_config());
    Ok(config)
}
