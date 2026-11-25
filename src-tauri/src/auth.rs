use anyhow::{bail, Context, Result};
use keepass::db::{Entry, EntryMut};
use russh::keys::{self, ssh_key::certificate::CertType, Certificate, HashAlg, PrivateKey};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

const METHOD: &str = "Harbor.AuthMethod";
const KEY: &str = "Harbor.PrivateKey";
const PASSPHRASE: &str = "Harbor.KeyPassphrase";
const CERT: &str = "Harbor.Certificate";

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Method {
    #[default]
    Password,
    PrivateKey,
    Certificate,
}
impl Method {
    pub fn from_entry(entry: &Entry) -> Result<Self> {
        match entry.get(METHOD).unwrap_or("password") {
            "password" => Ok(Self::Password),
            "privateKey" => Ok(Self::PrivateKey),
            "certificate" => Ok(Self::Certificate),
            _ => bail!("Unsupported saved authentication method"),
        }
    }
    fn field(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "privateKey",
            Self::Certificate => "certificate",
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub method: Method,
    pub private_key: Option<String>,
    pub private_key_path: Option<String>,
    pub passphrase: Option<String>,
    pub certificate: Option<String>,
    pub certificate_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub fingerprint: String,
    pub certificate_id: Option<String>,
    pub principals: Vec<String>,
    pub valid_after: Option<u64>,
    pub valid_before: Option<u64>,
    pub ca_fingerprint: Option<String>,
}

pub enum Material {
    Password(Zeroizing<String>),
    Key {
        private_key: Zeroizing<String>,
        passphrase: Zeroizing<String>,
        certificate: Option<String>,
    },
}

fn read_text(
    path: Option<String>,
    text: Option<String>,
    existing: Option<&str>,
) -> Result<Zeroizing<String>> {
    let text = text.map(Zeroizing::new);
    if let Some(path) = path.filter(|p| !p.is_empty()) {
        if text.is_some() {
            bail!("Choose either an identity file or pasted contents");
        }
        let mut bytes = Zeroizing::new(Vec::new());
        File::open(path)
            .context("Could not open the identity file")?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            bail!("Identity files must be smaller than 1 MiB");
        }
        return Ok(Zeroizing::new(
            std::str::from_utf8(&bytes)
                .context(
                    "Use a text-format OpenSSH, PEM, or PPK private key / OpenSSH certificate",
                )?
                .to_string(),
        ));
    }
    text.or_else(|| existing.map(|s| Zeroizing::new(s.to_string())))
        .context(
            "Choose a private key and, for certificate authentication, its OpenSSH certificate",
        )
}

pub fn parse_key(
    private_key: &str,
    passphrase: &str,
    certificate: Option<&str>,
    check_time: bool,
) -> Result<(Arc<PrivateKey>, Option<Certificate>, Identity)> {
    let key = keys::decode_secret_key(
        private_key,
        if passphrase.is_empty() {
            None
        } else {
            Some(passphrase)
        },
    )
    .map_err(|_| {
        anyhow::anyhow!("Could not read the private key. Check its format and passphrase.")
    })?;
    if matches!(
        key.algorithm(),
        keys::Algorithm::SkEd25519 | keys::Algorithm::SkEcdsaSha2NistP256
    ) {
        bail!("FIDO / security-key SSH identities require an external signer and are not supported in this version");
    }
    let fingerprint = key.public_key().fingerprint(HashAlg::Sha256).to_string();
    let mut identity = Identity {
        fingerprint,
        certificate_id: None,
        principals: vec![],
        valid_after: None,
        valid_before: None,
        ca_fingerprint: None,
    };
    let certificate=certificate.map(|text|->Result<Certificate> {
        let cert=Certificate::from_openssh(text.trim()).context("Not a valid OpenSSH certificate (X.509 / TLS certificates are not SSH user certificates)")?;
        if cert.cert_type()!=CertType::User {bail!("Choose an SSH user certificate, not a server host certificate");}
        if cert.public_key()!=key.public_key().key_data() {bail!("The certificate does not match this private key");}
        // This checks the certificate's integrity; the SSH server decides which CA/principals it trusts.
        cert.verify_signature().context("The certificate signature is invalid")?;
        if check_time {
            let now=SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            if now<cert.valid_after(){bail!("The SSH certificate is not valid yet");}
            if now>=cert.valid_before(){bail!("The SSH certificate has expired; import a renewed certificate");}
        }
        identity.certificate_id=Some(cert.key_id().into());
        identity.principals=cert.valid_principals().to_vec();
        identity.valid_after=Some(cert.valid_after());identity.valid_before=Some(cert.valid_before());
        identity.ca_fingerprint=Some(cert.signature_key().fingerprint(HashAlg::Sha256).to_string());
        Ok(cert)
    }).transpose()?;
    Ok((Arc::new(key), certificate, identity))
}

pub fn update(entry: &mut EntryMut<'_>, mut update: Option<Update>) -> Result<Method> {
    let Some(mut update) = update.take() else {
        return Method::from_entry(entry);
    };
    // Immediately wrap all secret input buffers, including paths read below, for cleanup on error.
    let passphrase = update.passphrase.take().map(Zeroizing::new);
    let private_key = update.private_key.take().map(Zeroizing::new);
    if update.method == Method::Password {
        for field in [KEY, PASSPHRASE, CERT, "Harbor.Identity"] {
            entry.fields.remove(field);
        }
    } else {
        let key = read_text(
            update.private_key_path,
            private_key.map(|s| s.to_string()),
            entry.get(KEY),
        )?;
        let passphrase = passphrase
            .unwrap_or_else(|| Zeroizing::new(entry.get(PASSPHRASE).unwrap_or("").into()));
        let cert = if update.method == Method::Certificate {
            Some(read_text(
                update.certificate_path,
                update.certificate,
                entry.get(CERT),
            )?)
        } else {
            None
        };
        let (_, _, identity) = parse_key(
            &key,
            &passphrase,
            cert.as_deref().map(|s| s.as_str()),
            false,
        )?;
        entry.set_protected(KEY, key.as_str());
        entry.set_protected(PASSPHRASE, passphrase.as_str());
        entry.set_unprotected("Harbor.Identity", serde_json::to_string(&identity)?);
        if let Some(cert) = cert {
            entry.set_protected(CERT, cert.as_str());
        } else {
            entry.fields.remove(CERT);
        }
        entry.fields.remove(keepass::db::fields::PASSWORD);
    }
    entry.set_unprotected(METHOD, update.method.field());
    Ok(update.method)
}

pub fn identity(entry: &Entry) -> Result<Option<Identity>> {
    entry
        .get("Harbor.Identity")
        .map(|json| serde_json::from_str(json).context("Invalid saved identity metadata"))
        .transpose()
}
pub fn material(entry: &Entry) -> Result<Material> {
    let method = Method::from_entry(entry)?;
    if method == Method::Password {
        return Ok(Material::Password(Zeroizing::new(
            entry.get_password().unwrap_or("").into(),
        )));
    }
    Ok(Material::Key {
        private_key: Zeroizing::new(
            entry
                .get(KEY)
                .context("Saved private key is missing")?
                .into(),
        ),
        passphrase: Zeroizing::new(entry.get(PASSPHRASE).unwrap_or("").into()),
        certificate: if method == Method::Certificate {
            Some(
                entry
                    .get(CERT)
                    .context("Saved certificate is missing")?
                    .into(),
            )
        } else {
            None
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn certificate_matching_types_dates_and_encrypted_keys() -> Result<()> {
        use keys::ssh_key::{certificate, LineEnding};
        let key = PrivateKey::random(&mut rand::rng(), keys::Algorithm::Ed25519)?;
        let ca = PrivateKey::random(&mut rand::rng(), keys::Algorithm::Ed25519)?;
        let other = PrivateKey::random(&mut rand::rng(), keys::Algorithm::Ed25519)?;
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let cert = |kind, after, before| -> Result<String> {
            let mut builder =
                certificate::Builder::new(vec![7; 32], key.public_key(), after, before)?;
            builder
                .cert_type(kind)?
                .valid_principal("tester")?
                .key_id("harbor-test")?;
            Ok(builder.sign(&ca)?.to_openssh()?)
        };
        let text = key.to_openssh(LineEnding::LF)?;
        let valid = cert(CertType::User, now - 60, now + 3600)?;
        let (_, _, info) = parse_key(&text, "", Some(&valid), true)?;
        assert_eq!(info.certificate_id.as_deref(), Some("harbor-test"));
        assert!(parse_key(&other.to_openssh(LineEnding::LF)?, "", Some(&valid), true).is_err());
        assert!(parse_key(
            &text,
            "",
            Some(&cert(CertType::Host, now - 60, now + 3600)?),
            false
        )
        .is_err());
        assert!(parse_key(
            &text,
            "",
            Some(&cert(CertType::User, now - 3600, now - 1)?),
            true
        )
        .is_err());
        assert!(parse_key(
            &text,
            "",
            Some(&cert(CertType::User, now + 60, now + 3600)?),
            true
        )
        .is_err());
        let encrypted = key
            .encrypt(&mut rand::rng(), "key-passphrase")?
            .to_openssh(LineEnding::LF)?;
        assert!(parse_key(&encrypted, "wrong", None, true).is_err());
        assert!(parse_key(&encrypted, "key-passphrase", Some(&valid), true).is_ok());
        Ok(())
    }
}
