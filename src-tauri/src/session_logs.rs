use aes_gcm::{
    aead::{Aead, Payload},
    Aes256Gcm, KeyInit,
};
use anyhow::{bail, Context, Result};
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"HBRLOG01";
const MAX_FILE: u64 = 32 * 1024 * 1024;
const MAX_RECORD: usize = 128 * 1024;

pub struct Access {
    pub root: PathBuf,
    pub key: Zeroizing<Vec<u8>>,
}
impl Access {
    pub fn directory(&self) -> PathBuf {
        let hash = Sha256::digest(self.key.as_slice());
        self.root
            .join(hash.iter().map(|b| format!("{b:02x}")).collect::<String>())
    }
    fn path(&self, id: &str) -> Result<PathBuf> {
        let id = Uuid::parse_str(id).context("Invalid session log ID")?;
        Ok(self.directory().join(format!("{id}.hlog")))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub started_at: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Record {
    Start { session: Summary },
    Output { at: u64, bytes: Vec<u8> },
    End { at: u64, reason: String },
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLog {
    pub session: Summary,
    pub records: Vec<Record>,
    pub complete: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    pub id: String,
    pub session: Option<Summary>,
    pub error: Option<String>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn cipher(key: &[u8], salt: &[u8], id: &str) -> Result<Aes256Gcm> {
    let mut derived = Zeroizing::new([0u8; 32]);
    Hkdf::<Sha256>::new(Some(salt), key)
        .expand(
            format!("Harbor.SessionLog.v1:{id}").as_bytes(),
            derived.as_mut(),
        )
        .map_err(|_| anyhow::anyhow!("Could not derive log key"))?;
    Aes256Gcm::new_from_slice(derived.as_slice()).map_err(|_| anyhow::anyhow!("Invalid log key"))
}
fn nonce(index: u64) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&index.to_be_bytes());
    nonce
}
fn aad(id: &str, salt: &[u8], index: u64) -> Vec<u8> {
    [MAGIC.as_slice(), id.as_bytes(), salt, &index.to_be_bytes()].concat()
}

pub struct Writer {
    file: File,
    cipher: Aes256Gcm,
    salt: [u8; 32],
    id: String,
    index: u64,
    size: u64,
    failed: bool,
}
impl Writer {
    pub fn start(access: Access, name: &str, host: &str, port: u16) -> Result<Self> {
        let id = Uuid::new_v4().to_string();
        let mut salt = [0; 32];
        getrandom::fill(&mut salt)?;
        fs::create_dir_all(access.directory())?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(access.path(&id)?)?;
        file.write_all(MAGIC)?;
        file.write_all(&salt)?;
        let mut writer = Self {
            file,
            cipher: cipher(&access.key, &salt, &id)?,
            salt,
            id: id.clone(),
            index: 0,
            size: 40,
            failed: false,
        };
        writer.record(&Record::Start {
            session: Summary {
                id,
                name: name.into(),
                host: host.into(),
                port,
                started_at: now(),
            },
        })?;
        Ok(writer)
    }
    fn record(&mut self, record: &Record) -> Result<()> {
        anyhow::ensure!(
            !self.failed,
            "Session log writer is unavailable after a recording error"
        );
        let result = self.record_inner(record);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn record_inner(&mut self, record: &Record) -> Result<()> {
        let plaintext = Zeroizing::new(serde_json::to_vec(record)?);
        anyhow::ensure!(
            plaintext.len() + 16 <= MAX_RECORD,
            "Session log record is too large"
        );
        let encrypted = self
            .cipher
            .encrypt(
                (&nonce(self.index)).into(),
                Payload {
                    msg: &plaintext,
                    aad: &aad(&self.id, &self.salt, self.index),
                },
            )
            .map_err(|_| anyhow::anyhow!("Session log encryption failed"))?;
        anyhow::ensure!(
            self.size + encrypted.len() as u64 + 4 <= MAX_FILE,
            "Encrypted session log reached the 32 MiB limit; reconnect to start a new log"
        );
        self.file
            .write_all(&(encrypted.len() as u32).to_be_bytes())?;
        self.file.write_all(&encrypted)?;
        self.size += encrypted.len() as u64 + 4;
        self.index += 1;
        self.file.flush()?;
        Ok(())
    }
    pub fn output(&mut self, bytes: &[u8]) -> Result<()> {
        for chunk in bytes.chunks(8192) {
            self.record(&Record::Output {
                at: now(),
                bytes: chunk.to_vec(),
            })?;
        }
        Ok(())
    }
    pub fn finish(&mut self, reason: &str) -> Result<()> {
        self.record(&Record::End {
            at: now(),
            reason: reason.into(),
        })?;
        self.file.sync_all()?;
        Ok(())
    }
}
struct Reader {
    file: File,
    cipher: Aes256Gcm,
    salt: [u8; 32],
    id: String,
    index: u64,
}
impl Reader {
    fn open(access: &Access, id: &str) -> Result<Self> {
        let id = Uuid::parse_str(id)?.to_string();
        let path = access.path(&id)?;
        anyhow::ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "Log symlinks are not supported"
        );
        let mut file = File::open(path)?;
        anyhow::ensure!(
            file.metadata()?.len() <= MAX_FILE,
            "Session log exceeds size limit"
        );
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        anyhow::ensure!(&magic == MAGIC, "Unsupported session log format");
        let mut salt = [0; 32];
        file.read_exact(&mut salt)?;
        Ok(Self {
            file,
            cipher: cipher(&access.key, &salt, &id)?,
            salt,
            id,
            index: 0,
        })
    }
    fn next(&mut self) -> Result<Option<Record>> {
        let mut length = [0; 4];
        if self.file.read(&mut length[..1])? == 0 {
            return Ok(None);
        }
        self.file
            .read_exact(&mut length[1..])
            .context("Incomplete session log record")?;
        let length = u32::from_be_bytes(length) as usize;
        anyhow::ensure!(
            (16..=MAX_RECORD).contains(&length),
            "Invalid session log record length"
        );
        let mut encrypted = vec![0; length];
        self.file
            .read_exact(&mut encrypted)
            .context("Incomplete session log record")?;
        let plaintext = Zeroizing::new(
            self.cipher
                .decrypt(
                    (&nonce(self.index)).into(),
                    Payload {
                        msg: &encrypted,
                        aad: &aad(&self.id, &self.salt, self.index),
                    },
                )
                .map_err(|_| {
                    anyhow::anyhow!("Log authentication failed: incorrect key or damaged log")
                })?,
        );
        self.index += 1;
        Ok(Some(serde_json::from_slice(&plaintext)?))
    }
    fn summary(&mut self) -> Result<Summary> {
        match self.next()? {
            Some(Record::Start { session }) if session.id == self.id => Ok(session),
            _ => bail!("Session log header is missing or invalid"),
        }
    }
}
pub fn list(access: &Access) -> Result<Vec<ListItem>> {
    let directory = access.directory();
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("hlog") {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| Uuid::parse_str(s).is_ok())
        else {
            continue;
        };
        let result = Reader::open(access, id).and_then(|mut reader| reader.summary());
        items.push(match result {
            Ok(session) => ListItem {
                id: id.into(),
                session: Some(session),
                error: None,
            },
            Err(e) => ListItem {
                id: id.into(),
                session: None,
                error: Some(e.to_string()),
            },
        });
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.session.as_ref().map_or(0, |s| s.started_at)));
    Ok(items)
}
pub fn read(access: &Access, id: &str) -> Result<SavedLog> {
    let mut reader = Reader::open(access, id)?;
    let session = reader.summary()?;
    let mut records = Vec::new();
    let mut complete = false;
    while let Some(record) = reader.next()? {
        anyhow::ensure!(!complete, "Unexpected data after session log end");
        match &record {
            Record::Start { .. } => bail!("Unexpected session log header"),
            Record::End { .. } => complete = true,
            _ => {}
        }
        records.push(record);
    }
    Ok(SavedLog {
        session,
        records,
        complete,
    })
}
pub fn delete(access: &Access, id: &str) -> Result<()> {
    fs::remove_file(access.path(id)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn access(root: &Path) -> Access {
        Access {
            root: root.into(),
            key: Zeroizing::new(vec![42; 32]),
        }
    }
    #[test]
    fn encrypted_log_roundtrip_tamper_truncation_and_vault_isolation() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut writer = Writer::start(access(dir.path()), "Private server", "secret.example", 22)?;
        let id = writer.id.clone();
        writer.output(b"private-terminal-output\r\n")?;
        assert!(!read(&access(dir.path()), &id)?.complete);
        writer.finish("Session closed")?;
        drop(writer);
        let path = access(dir.path()).path(&id)?;
        let original = fs::read(&path)?;
        for secret in [b"secret.example".as_slice(), b"private-terminal-output"] {
            assert!(!original.windows(secret.len()).any(|b| b == secret));
        }
        let saved = read(&access(dir.path()), &id)?;
        assert!(saved.complete);
        assert_eq!(saved.session.host, "secret.example");
        assert!(
            matches!(&saved.records[0],Record::Output{bytes,..} if bytes == b"private-terminal-output\r\n")
        );
        assert_eq!(list(&access(dir.path()))?.len(), 1);
        assert!(read(
            &Access {
                root: dir.path().into(),
                key: Zeroizing::new(vec![1; 32])
            },
            &id
        )
        .is_err());
        let mut damaged = original.clone();
        *damaged.last_mut().unwrap() ^= 1;
        fs::write(&path, damaged)?;
        assert!(read(&access(dir.path()), &id).is_err());
        fs::write(&path, &original[..original.len() - 1])?;
        assert!(read(&access(dir.path()), &id).is_err());
        let first_len = u32::from_be_bytes(original[40..44].try_into().unwrap()) as usize + 4;
        let second_start = 40 + first_len;
        let second_len =
            u32::from_be_bytes(original[second_start..second_start + 4].try_into().unwrap())
                as usize
                + 4;
        let third_start = second_start + second_len;
        let mut reordered = original[..second_start].to_vec();
        reordered.extend_from_slice(&original[third_start..]);
        reordered.extend_from_slice(&original[second_start..third_start]);
        fs::write(&path, reordered)?;
        assert!(read(&access(dir.path()), &id).is_err());
        fs::write(&path, &original)?;
        let other_id = Uuid::new_v4().to_string();
        fs::copy(&path, access(dir.path()).path(&other_id)?)?;
        assert!(read(&access(dir.path()), &other_id).is_err());
        assert!(access(dir.path()).path("../../bad").is_err());
        let mut limited = Writer::start(access(dir.path()), "Limit test", "local", 22)?;
        limited.size = MAX_FILE;
        assert!(limited
            .output(b"over limit")
            .unwrap_err()
            .to_string()
            .contains("32 MiB"));
        assert!(!read(&access(dir.path()), &limited.id)?.complete);
        let limited_path = access(dir.path()).path(&limited.id)?;
        let before = fs::read(&limited_path)?;
        assert!(limited
            .finish("must not reuse a failed encryption nonce")
            .is_err());
        assert_eq!(before, fs::read(limited_path)?);
        delete(&access(dir.path()), &id)?;
        assert!(!path.exists());
        Ok(())
    }
}
