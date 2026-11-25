use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub supported: bool,
    pub enabled: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    version: u32,
    pub credential: String,
    challenge: [u8; 32],
    nonce: [u8; 12],
    ciphertext: Vec<u8>,
}

#[cfg(any(windows, test))]
mod wrapping {
    use super::*;
    use aes_gcm::{
        aead::{Aead, Payload},
        Aes256Gcm, KeyInit,
    };
    use sha2::{Digest, Sha256};
    fn key(signature: &[u8]) -> Zeroizing<Vec<u8>> {
        let mut hash = Sha256::new();
        hash.update(b"Harbor.Hello.WrappingKey.v1");
        hash.update(signature);
        Zeroizing::new(hash.finalize().to_vec())
    }
    fn aad(path: &str, e: &Envelope) -> Vec<u8> {
        format!("Harbor.Hello.v{}\0{}\0{}", e.version, path, e.credential).into_bytes()
    }
    pub(super) fn seal(
        path: &str,
        secret: &[u8],
        signature: &[u8],
        credential: String,
        challenge: [u8; 32],
    ) -> Result<Envelope> {
        let mut nonce = [0; 12];
        getrandom::fill(&mut nonce)?;
        let mut e = Envelope {
            version: 1,
            credential,
            challenge,
            nonce,
            ciphertext: vec![],
        };
        let key = key(signature);
        let cipher =
            Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow::anyhow!("Invalid wrapping key"))?;
        e.ciphertext = cipher
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: secret,
                    aad: &aad(path, &e),
                },
            )
            .map_err(|_| anyhow::anyhow!("Could not protect the vault second factor"))?;
        Ok(e)
    }
    pub(super) fn unseal(path: &str, e: &Envelope, signature: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
        if e.version != 1 || e.ciphertext.len() > 65536 + 16 {
            bail!("Unsupported Windows Hello unlock record; use the master password and recovery key file");
        }
        let key = key(signature);
        let cipher =
            Aes256Gcm::new_from_slice(&key).map_err(|_| anyhow::anyhow!("Invalid wrapping key"))?;
        let bytes=Zeroizing::new(cipher.decrypt((&e.nonce).into(),Payload {msg:&e.ciphertext,aad:&aad(path,e)}).map_err(|_|anyhow::anyhow!("Windows Hello could not recover the second factor. Use your master password and recovery key file."))?);
        Ok(bytes)
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use anyhow::Context;
    use windows::{
        core::{Array, HSTRING},
        Security::{
            Credentials::{KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus},
            Cryptography::CryptographicBuffer,
        },
        Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
    };
    struct Apartment;
    impl Apartment {
        fn new() -> Result<Self> {
            unsafe {
                RoInitialize(RO_INIT_MULTITHREADED)?;
            }
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    fn check(status: KeyCredentialStatus) -> Result<()> {
        if status == KeyCredentialStatus::Success {
            return Ok(());
        }
        if status == KeyCredentialStatus::UserCanceled
            || status == KeyCredentialStatus::UserPrefersPassword
        {
            bail!("Windows Hello was cancelled. Use the master password and recovery key file instead.");
        }
        bail!("Windows Hello could not authorize this operation (status {}). Use the master password and recovery key file.",status.0)
    }
    fn valid_name(name: &str) -> Result<()> {
        let raw = name
            .strip_prefix("Harbor.VaultUnlock.")
            .context("Invalid Harbor Hello credential identifier")?;
        uuid::Uuid::parse_str(raw).context("Invalid Harbor Hello credential identifier")?;
        Ok(())
    }
    pub fn available() -> Result<bool> {
        let _apartment = Apartment::new()?;
        Ok(KeyCredentialManager::IsSupportedAsync()?.join()?)
    }
    fn sign(name: &str, challenge: &[u8; 32], create: bool) -> Result<Zeroizing<Vec<u8>>> {
        valid_name(name)?;
        let name = HSTRING::from(name);
        let result = if create {
            KeyCredentialManager::RequestCreateAsync(
                &name,
                KeyCredentialCreationOption::FailIfExists,
            )?
            .join()?
        } else {
            KeyCredentialManager::OpenAsync(&name)?.join()?
        };
        check(result.Status()?)?;
        // Never replace a missing Hello key during unlock. Only enrollment creates one.
        let mut message = b"Harbor vault unlock signature v1\0".to_vec();
        message.extend_from_slice(challenge);
        let result = result
            .Credential()?
            .RequestSignAsync(&CryptographicBuffer::CreateFromByteArray(&message)?)?
            .join()?;
        check(result.Status()?)?;
        let mut bytes = Array::new();
        CryptographicBuffer::CopyToByteArray(&result.Result()?, &mut bytes)?;
        let signature = Zeroizing::new(bytes.to_vec());
        // WinRT's intermediate result is released immediately; the Rust copy is zeroized.
        Ok(signature)
    }
    pub fn enroll(path: &str, secret: &[u8]) -> Result<Envelope> {
        if secret.len() > 65536 {
            bail!("Key files larger than 64 KiB are not supported by Windows Hello");
        }
        let _apartment = Apartment::new()?;
        if !KeyCredentialManager::IsSupportedAsync()?.join()? {
            bail!("Windows Hello is not configured or supported for this Windows account");
        }
        let name = format!("Harbor.VaultUnlock.{}", uuid::Uuid::new_v4());
        let mut challenge = [0; 32];
        getrandom::fill(&mut challenge)?;
        let result = (|| {
            let signature = sign(&name, &challenge, true)?;
            wrapping::seal(path, secret, &signature, name.clone(), challenge)
        })();
        if result.is_err() {
            let _ =
                KeyCredentialManager::DeleteAsync(&HSTRING::from(&name)).and_then(|op| op.join());
        }
        result
    }
    pub fn unlock(path: &str, envelope: &Envelope) -> Result<Zeroizing<Vec<u8>>> {
        let _apartment = Apartment::new()?;
        if envelope.version != 1 {
            bail!("Unsupported Hello record; use your master password and recovery key file");
        }
        let signature = sign(&envelope.credential, &envelope.challenge, false)?;
        wrapping::unseal(path, envelope, &signature)
    }
    pub fn delete(envelope: &Envelope) -> Result<()> {
        valid_name(&envelope.credential)?;
        let _apartment = Apartment::new()?;
        KeyCredentialManager::DeleteAsync(&HSTRING::from(&envelope.credential))?.join()?;
        Ok(())
    }
}

#[cfg(windows)]
pub use native::{available, delete, enroll, unlock};
#[cfg(not(windows))]
pub fn available() -> Result<bool> {
    Ok(false)
}
#[cfg(not(windows))]
pub fn enroll(_path: &str, _secret: &[u8]) -> Result<Envelope> {
    bail!("Windows Hello is available only on Windows")
}
#[cfg(not(windows))]
pub fn unlock(_path: &str, _envelope: &Envelope) -> Result<Zeroizing<Vec<u8>>> {
    bail!("Use your master password and recovery key file on this platform")
}
#[cfg(not(windows))]
pub fn delete(_envelope: &Envelope) -> Result<()> {
    Ok(())
}

#[cfg(test)]
pub(crate) fn test_envelope(path: &str, factor: &[u8]) -> Envelope {
    wrapping::seal(
        path,
        factor,
        b"test-only fake Hello signature",
        format!("Harbor.VaultUnlock.{}", uuid::Uuid::new_v4()),
        [3; 32],
    )
    .unwrap()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hello_envelope_binds_signature_path_and_authenticated_ciphertext() -> Result<()> {
        let secret = b"test-only recovery key material";
        let envelope = wrapping::seal(
            "vault-a",
            secret,
            b"test-only signed challenge",
            format!("Harbor.VaultUnlock.{}", uuid::Uuid::new_v4()),
            [5; 32],
        )?;
        let encoded = serde_json::to_string(&envelope)?;
        assert!(!encoded.contains(std::str::from_utf8(secret)?));
        assert_eq!(
            wrapping::unseal("vault-a", &envelope, b"test-only signed challenge")?.as_slice(),
            secret
        );
        assert!(wrapping::unseal("vault-a", &envelope, b"different signature").is_err());
        assert!(wrapping::unseal("vault-b", &envelope, b"test-only signed challenge").is_err());
        let mut corrupt = envelope;
        corrupt.ciphertext[0] ^= 1;
        assert!(wrapping::unseal("vault-a", &corrupt, b"test-only signed challenge").is_err());
        Ok(())
    }
}
