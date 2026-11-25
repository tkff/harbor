use crate::auth;
use crate::forwarding::{self, Rule};
use crate::hello;
use anyhow::{bail, Context, Result};
use keepass::{
    config::KdfConfig,
    db::{fields, EntryId},
    Database, DatabaseKey,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub folder: String,
    #[serde(default)]
    pub startup_snippet: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub forwards: Vec<Rule>,
    #[serde(default)]
    pub auth_method: auth::Method,
    pub identity: Option<auth::Identity>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> SaveProfile {
        SaveProfile {
            id: None,
            name: "Test server".into(),
            host: "127.0.0.1".into(),
            port: 2222,
            folder: "Lab".into(),
            startup_snippet: None,
            tags: vec!["production".into(), "linux".into()],
            username: "harbor-test-user".into(),
            password: Some("test-ssh-secret".into()),
            forwards: vec![],
            authentication: None,
        }
    }

    #[test]
    fn snippets_encrypted_portable_and_startup_references() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("snippets.kdbx");
        let mut store = Storage::open(&dir.path().join("local.db"))?;
        assert!(store.snippets().is_err());
        store.unlock(&path, Zeroizing::new("test-master-password".into()), true)?;
        let make = |id: Option<String>, body: &str| SaveSnippet {
            id,
            name: "Server check".into(),
            group: "Maintenance".into(),
            tags: vec![" Linux ".into(), "linux".into()],
            body: body.into(),
        };
        let body = "echo harbor-snippet-private-marker\r\ndf -h\r\n";
        let saved = store.save_snippet(make(None, body))?;
        let id = saved[0].id.clone();
        assert_eq!(saved[0].tags, vec!["linux"]);
        assert_eq!(saved[0].body, body.replace("\r\n", "\n"));
        let mut host = profile();
        host.startup_snippet = Some(id.clone());
        let state = store.save_profile(host)?;
        let host_id = state.profiles[0].id.clone();
        assert_eq!(
            store.startup_command(&state.profiles[0])?.unwrap().as_str(),
            "echo harbor-snippet-private-marker\ndf -h\r"
        );
        assert!(store
            .delete_snippet(&id)
            .unwrap_err()
            .to_string()
            .contains("used at startup"));
        assert!(store
            .save_snippet(make(Some(id.clone()), "\u{1b}[2J"))
            .is_err());
        assert!(store
            .save_snippet(make(Some(host_id.clone()), "echo must not overwrite host"))
            .is_err());
        for file in [&path, &dir.path().join("local.db")] {
            let bytes = fs::read(file)?;
            assert!(!bytes
                .windows(b"harbor-snippet-private-marker".len())
                .any(|b| b == b"harbor-snippet-private-marker"));
        }
        store.lock();
        assert!(store.snippet(&id).is_err());
        let mut fresh = Storage::open(&dir.path().join("fresh.db"))?;
        let state = fresh.unlock(&path, Zeroizing::new("test-master-password".into()), false)?;
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(
            state.profiles[0].startup_snippet.as_deref(),
            Some(id.as_str())
        );
        assert_eq!(fresh.snippet(&id)?.group, "Maintenance");
        fresh.save_snippet(make(Some(id.clone()), "echo updated"))?;
        assert_eq!(
            fresh
                .startup_command(&fresh.status()?.profiles[0])?
                .unwrap()
                .as_str(),
            "echo updated\r"
        );
        let mut host = profile();
        host.id = Some(state.profiles[0].id.clone());
        host.password = None;
        fresh.save_profile(host)?;
        assert!(fresh.delete_snippet(&id)?.is_empty());
        assert_eq!(fresh.status()?.profiles.len(), 1);
        let mut invalid = profile();
        invalid.startup_snippet = Some(id);
        assert!(fresh.save_profile(invalid).is_err());
        Ok(())
    }

    #[test]
    fn legacy_index_migration_and_tag_validation() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("legacy.db");
        let sql = Connection::open(&path)?;
        sql.execute_batch("CREATE TABLE profiles(id TEXT PRIMARY KEY,vault TEXT NOT NULL,entry TEXT NOT NULL,name TEXT NOT NULL,host TEXT NOT NULL,port INTEGER NOT NULL,folder TEXT NOT NULL);
            INSERT INTO profiles VALUES('old','vault','entry','Old host','localhost',22,'Work');")?;
        drop(sql);
        for _ in 0..2 {
            let store = Storage::open(&path)?;
            let (group, tags, forwards): (String, String, String) = store.sql.query_row(
                "SELECT folder,tags,forwards FROM profiles WHERE id='old'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            assert_eq!(
                (group, tags, forwards),
                ("Work".into(), "[]".into(), "[]".into())
            );
        }
        assert_eq!(
            normalize_tags(&[" PROD ".into(), "prod".into(), "Linux".into(), "".into()])?,
            vec!["linux", "prod"]
        );
        assert!(normalize_tags(&["bad;tag".into()]).is_err());
        assert!(normalize_tags(&["x".repeat(41)]).is_err());
        assert!(normalize_tags(&(0..21).map(|i| i.to_string()).collect::<Vec<_>>()).is_err());
        Ok(())
    }

    #[test]
    fn vault_roundtrip_backup_conflict_and_portability() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("vault.kdbx");
        let mut store = Storage::open(&dir.path().join("local.db"))?;
        store.unlock(&path, Zeroizing::new("test-master-password".into()), true)?;
        let mut saved = profile();
        saved.forwards = vec![Rule {
            id: Uuid::new_v4().to_string(),
            direction: forwarding::Direction::Remote,
            bind_address: "127.0.0.1".into(),
            bind_port: 9000,
            target_host: "localhost".into(),
            target_port: 3000,
            auto_start: false,
        }];
        let rules = saved.forwards.clone();
        let state = store.save_profile(saved)?;
        assert_eq!(state.profiles[0].forwards, rules);
        assert_eq!(state.profiles[0].tags, vec!["linux", "production"]);
        assert_eq!(state.profiles[0].folder, "Lab");
        let id = state.profiles[0].id.clone();
        let bytes = fs::read(&path)?;
        for secret in [
            "harbor-test-user",
            "test-ssh-secret",
            "test-master-password",
        ] {
            assert!(!bytes.windows(secret.len()).any(|s| s == secret.as_bytes()));
            assert!(!fs::read(dir.path().join("local.db"))?
                .windows(secret.len())
                .any(|s| s == secret.as_bytes()));
        }
        assert!(path.with_extension("kdbx.bak").exists());
        let (_, username, password) = store.credentials(&id)?;
        assert_eq!(username.as_str(), "harbor-test-user");
        assert_eq!(password.as_str(), "test-ssh-secret");
        let mut updated = profile();
        updated.id = Some(id.clone());
        updated.name = "Renamed".into();
        updated.password = None;
        updated.forwards = rules.clone();
        store.save_profile(updated)?;
        assert_eq!(store.credentials(&id)?.2.as_str(), "test-ssh-secret");
        store.lock();
        assert!(store.credentials(&id).is_err());
        assert!(store
            .unlock(&path, Zeroizing::new("wrong password".into()), false)
            .is_err());
        // A fresh local database rebuilds connections from the copied vault.
        let mut second = Storage::open(&dir.path().join("second.db"))?;
        let state = second.unlock(&path, Zeroizing::new("test-master-password".into()), false)?;
        assert_eq!(state.profiles.len(), 1);
        assert_eq!(state.profiles[0].name, "Renamed");
        assert_eq!(state.profiles[0].forwards, rules);
        assert_eq!(state.profiles[0].tags, vec!["linux", "production"]);
        assert_eq!(state.profiles[0].folder, "Lab");
        let second_id = state.profiles[0].id.clone();
        let mut external = fs::read(&path)?;
        external.push(0);
        fs::write(&path, &external)?;
        assert!(second
            .delete_profile(&second_id)
            .unwrap_err()
            .to_string()
            .contains("changed outside"));
        assert_eq!(fs::read(&path)?, external);
        // Restore valid content, reopen, and verify deletion persists.
        fs::write(&path, &external[..external.len() - 1])?;
        second.lock();
        second.unlock(&path, Zeroizing::new("test-master-password".into()), false)?;
        assert_eq!(second.delete_profile(&second_id)?.profiles.len(), 0);
        Ok(())
    }

    #[test]
    fn host_keys_and_create_never_overwrite_existing_file() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut store = Storage::open(&dir.path().join("local.db"))?;
        let path = dir.path().join("existing.kdbx");
        fs::write(&path, b"keep me")?;
        assert!(store
            .unlock(&path, Zeroizing::new("test-master-password".into()), true)
            .is_err());
        assert_eq!(fs::read(&path)?, b"keep me");
        assert!(store.known_key("HOST", 22)?.is_none());
        store.trust_key("HOST", 22, "SHA256:first")?;
        assert_eq!(
            store.known_key("host", 22)?.as_deref(),
            Some("SHA256:first")
        );
        assert!(store.trust_key("host", 22, "SHA256:changed").is_err());
        Ok(())
    }

    #[test]
    fn vault_second_factor_recovery_and_backup_require_both_factors() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("factor.kdbx");
        let recovery = dir.path().join("recovery.key");
        let mut store = Storage::open(&dir.path().join("local.db"))?;
        store.unlock(&path, Zeroizing::new("test-master-password".into()), true)?;
        store.save_profile(profile())?;
        assert!(store
            .hello_material(&path, "wrong password", &recovery)
            .is_err());
        let (canonical, factor, rekey) =
            store.hello_material(&path, "test-master-password", &recovery)?;
        assert!(rekey);
        assert_eq!(factor.len(), 32);
        save_recovery(&recovery, &path, &factor)?;
        assert!(save_recovery(&recovery, &path, b"must not overwrite").is_err());
        assert_eq!(read_key_file(&recovery)?.as_slice(), factor.as_slice());
        let envelope = hello::test_envelope(&canonical, &factor);
        assert!(
            store
                .commit_hello(&canonical, "test-master-password", &factor, &envelope, true)?
                .requires_key_file
        );
        assert!(store.hello_record(&canonical)?.is_some());
        store.lock();
        assert!(store
            .unlock(&path, Zeroizing::new("test-master-password".into()), false)
            .is_err());
        assert!(store
            .unlock_with_keyfile(
                &path,
                Zeroizing::new("wrong password".into()),
                false,
                Some(&factor)
            )
            .is_err());
        assert!(store
            .unlock_with_keyfile(
                &path,
                Zeroizing::new("test-master-password".into()),
                false,
                Some(&[8; 32])
            )
            .is_err());
        for file in [&path, &path.with_extension("kdbx.bak")] {
            assert!(Database::open(
                &mut fs::File::open(file)?,
                DatabaseKey::new().with_password("test-master-password")
            )
            .is_err());
            Database::open(
                &mut fs::File::open(file)?,
                DatabaseKey::new()
                    .with_password("test-master-password")
                    .with_keyfile(&mut factor.as_slice())?,
            )?;
        }
        let copy = dir.path().join("copied.kdbx");
        fs::copy(&path, &copy)?;
        let mut other = Storage::open(&dir.path().join("other-device.db"))?;
        assert!(other.hello_record(copy.to_str().unwrap())?.is_none());
        let state = other.unlock_with_keyfile(
            &copy,
            Zeroizing::new("test-master-password".into()),
            false,
            Some(&read_key_file(&recovery)?),
        )?;
        assert!(state.requires_key_file);
        assert_eq!(state.profiles.len(), 1);
        // Re-enrollment on a new PC must use the existing factor, not silently replace it.
        let (_, existing, rekey) =
            other.hello_material(&copy, "test-master-password", &recovery)?;
        assert!(!rekey);
        assert_eq!(existing.as_slice(), factor.as_slice());
        Ok(())
    }

    // Explicitly enabled with KEEPASSXC_CLI to test against an independent implementation.
    #[test]
    #[ignore = "Requires KEEPASSXC_CLI pointing to a working KeePassXC CLI"]
    fn keepassxc_interoperability() -> Result<()> {
        use std::process::{Command, Stdio};
        let cli =
            std::env::var_os("KEEPASSXC_CLI").context("Set KEEPASSXC_CLI to enable this test")?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("interop.kdbx");
        let mut store = Storage::open(&dir.path().join("local.db"))?;
        store.unlock(&path, Zeroizing::new("test-master-password".into()), true)?;
        store.save_profile(profile())?;
        let run = |args: &[&str]| -> Result<String> {
            let mut child = Command::new(&cli)
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            child
                .stdin
                .take()
                .context("CLI stdin missing")?
                .write_all(b"test-master-password\n")?;
            let out = child.wait_with_output()?;
            if !out.status.success() {
                bail!("KeePassXC failed: {}", String::from_utf8_lossy(&out.stderr));
            }
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        };
        let path_text = path.to_str().unwrap();
        let output = run(&["show", "-q", "-s", path_text, "Test server"])?;
        assert!(output.contains("harbor-test-user"));
        assert!(output.contains("test-ssh-secret"));
        // KeePassXC modifies a Harbor vault; Harbor then reads it back.
        run(&[
            "edit",
            "-q",
            "-u",
            "updated-by-keepassxc",
            path_text,
            "Test server",
        ])?;
        store.lock();
        let state = store.unlock(&path, Zeroizing::new("test-master-password".into()), false)?;
        assert_eq!(
            store.username(&state.profiles[0].id)?,
            "updated-by-keepassxc"
        );
        Ok(())
    }

    #[test]
    #[ignore = "Run via npm run test:interop"]
    fn independent_kdbx_interoperability() -> Result<()> {
        use std::process::Command;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("interop.kdbx");
        let mut store = Storage::open(&dir.path().join("local.db"))?;
        store.unlock(&path, Zeroizing::new("test-master-password".into()), true)?;
        store.save_profile(profile())?;
        store.save_snippet(SaveSnippet {
            id: None,
            name: "Portable snippet".into(),
            group: "Checks".into(),
            tags: vec!["linux".into()],
            body: "printf interop-snippet".into(),
        })?;
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/check-kdbx.mjs");
        let output = Command::new("node").arg(script).arg(&path).output()?;
        if !output.status.success() {
            bail!(
                "Independent KDBX check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        store.lock();
        let state = store.unlock(&path, Zeroizing::new("test-master-password".into()), false)?;
        let id = state.profiles[0].id.clone();
        assert_eq!(store.snippets()?[0].body, "printf updated-by-kdbxweb");
        assert_eq!(store.username(&id)?, "updated-by-kdbxweb");
        assert_eq!(state.profiles[0].tags, vec!["external-tag"]);
        assert_eq!(state.profiles[0].folder, "External group");
        assert_eq!(store.credentials(&id)?.2.as_str(), "test-ssh-secret");
        // Saving again must preserve independent custom fields and history.
        let mut updated = profile();
        updated.id = Some(id);
        updated.username = "updated-by-kdbxweb".into();
        updated.password = None;
        store.save_profile(updated)?;
        let vault = store.vault.as_ref().unwrap();
        let root = vault.db.root();
        let e = root.entries().next().unwrap();
        assert_eq!(e.get("Independent.Custom"), Some("preserve me"));
        assert!(!e.history.as_ref().unwrap().get_entries().is_empty());
        Ok(())
    }
    #[test]
    #[ignore = "Run via npm run test:interop"]
    fn independent_kdbx_interoperability_with_second_factor() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("two-factor.kdbx");
        let recovery = dir.path().join("recovery.key");
        let mut store = Storage::open(&dir.path().join("local.db"))?;
        store.unlock(&path, Zeroizing::new("test-master-password".into()), true)?;
        store.save_profile(profile())?;
        let (canonical, factor, _) =
            store.hello_material(&path, "test-master-password", &recovery)?;
        save_recovery(&recovery, &path, &factor)?;
        store.commit_hello(
            &canonical,
            "test-master-password",
            &factor,
            &hello::test_envelope(&canonical, &factor),
            true,
        )?;
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/check-kdbx.mjs");
        let output = std::process::Command::new("node")
            .arg(script)
            .arg(&path)
            .arg(&recovery)
            .output()?;
        if !output.status.success() {
            bail!(
                "Independent 2FA KDBX check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        store.lock();
        let state = store.unlock_with_keyfile(
            &path,
            Zeroizing::new("test-master-password".into()),
            false,
            Some(&factor),
        )?;
        assert_eq!(store.username(&state.profiles[0].id)?, "updated-by-kdbxweb");
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProfile {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub folder: String,
    #[serde(default)]
    pub startup_snippet: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub username: String,
    // None preserves the existing password on edit.
    pub password: Option<String>,
    #[serde(default)]
    pub forwards: Vec<Rule>,
    #[serde(default)]
    pub authentication: Option<auth::Update>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub unlocked: bool,
    pub vault_path: Option<String>,
    pub profiles: Vec<Profile>,
    pub requires_key_file: bool,
}

pub struct Vault {
    db: Database,
    key: DatabaseKey,
    path: PathBuf,
    digest: Vec<u8>,
    uses_key_file: bool,
}

pub struct Storage {
    pub sql: Connection,
    vault: Option<Vault>,
}

fn digest(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

impl Vault {
    fn write(&mut self, candidate: Database, create: bool) -> Result<()> {
        self.write_key(candidate, self.key.clone(), create, false)
    }
    fn write_key(
        &mut self,
        candidate: Database,
        key: DatabaseKey,
        create: bool,
        rekey: bool,
    ) -> Result<()> {
        if !create && digest(&fs::read(&self.path)?) != self.digest {
            bail!("Vault changed outside Harbor. Lock and reopen it before saving; no changes were written.");
        }
        let mut bytes = Vec::new();
        candidate
            .save(&mut bytes, key.clone())
            .context("Could not encrypt the vault")?;
        // Verify every serialized file before replacing the previous working copy.
        Database::open(&mut bytes.as_slice(), key.clone())
            .context("Vault write verification failed")?;
        let parent = self.path.parent().context("Vault needs a parent folder")?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        if create {
            temporary
                .persist_noclobber(&self.path)
                .map_err(|e| e.error)?;
        } else {
            // Encryption can take time; check again immediately before replacement.
            if digest(&fs::read(&self.path)?) != self.digest {
                bail!("Vault changed outside Harbor while saving. Lock and reopen it; no changes were written.");
            }
            if rekey {
                // Do not leave a password-only backup after enabling the second factor.
                let mut backup = tempfile::NamedTempFile::new_in(parent)?;
                backup.write_all(&bytes)?;
                backup.as_file().sync_all()?;
                backup
                    .persist(self.path.with_extension("kdbx.bak"))
                    .map_err(|e| e.error)?;
            } else {
                fs::copy(&self.path, self.path.with_extension("kdbx.bak"))?;
            }
            temporary.persist(&self.path).map_err(|e| e.error)?;
        }
        self.digest = digest(&bytes);
        self.db = candidate;
        self.key = key;
        Ok(())
    }
}

impl Storage {
    pub fn open(path: &Path) -> Result<Self> {
        let sql = Connection::open(path)?;
        sql.execute_batch("PRAGMA foreign_keys=ON;
            CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS profiles (
                id TEXT PRIMARY KEY, vault TEXT NOT NULL, entry TEXT NOT NULL,
                name TEXT NOT NULL, host TEXT NOT NULL, port INTEGER NOT NULL, folder TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS host_keys (
                host TEXT NOT NULL, port INTEGER NOT NULL, fingerprint TEXT NOT NULL,
                PRIMARY KEY(host, port));
            CREATE TABLE IF NOT EXISTS hello_records (path TEXT PRIMARY KEY, record TEXT NOT NULL);")?;
        let has_forwards = {
            let mut stmt = sql.prepare("PRAGMA table_info(profiles)")?;
            let columns = stmt
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            columns.iter().any(|c| c == "forwards")
        };
        if !has_forwards {
            sql.execute(
                "ALTER TABLE profiles ADD COLUMN forwards TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        let has_tags = {
            let mut stmt = sql.prepare("PRAGMA table_info(profiles)")?;
            let columns = stmt
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            columns.iter().any(|c| c == "tags")
        };
        if !has_tags {
            sql.execute(
                "ALTER TABLE profiles ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        Ok(Self { sql, vault: None })
    }

    pub fn status(&self) -> Result<Status> {
        let path = self
            .sql
            .query_row("SELECT value FROM settings WHERE key='vault'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()?;
        let profiles = if let Some(v) = &self.vault {
            let mut stmt = self.sql.prepare(
                "SELECT id,name,host,port,folder,forwards,tags FROM profiles WHERE vault=? ORDER BY folder,name",
            )?;
            let rows = stmt.query_map([v.path.to_string_lossy().as_ref()], |r| {
                Ok(Profile {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    host: r.get(2)?,
                    port: r.get(3)?,
                    folder: r.get(4)?,
                    startup_snippet: None,
                    tags: serde_json::from_str(&r.get::<_, String>(6)?).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            6,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    forwards: serde_json::from_str(&r.get::<_, String>(5)?).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?,
                    auth_method: auth::Method::Password,
                    identity: None,
                })
            })?;
            let mut profiles = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            for profile in &mut profiles {
                let entry_id = self.entry_id(&profile.id)?;
                let entry = v.db.entry(entry_id).context("Credential entry missing")?;
                profile.startup_snippet = entry
                    .get("Harbor.StartupSnippet")
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned);
                profile.auth_method = auth::Method::from_entry(&entry)?;
                profile.identity = auth::identity(&entry)?;
            }
            profiles
        } else {
            vec![]
        };
        Ok(Status {
            unlocked: self.vault.is_some(),
            vault_path: path,
            profiles,
            requires_key_file: self.vault.as_ref().is_some_and(|v| v.uses_key_file),
        })
    }

    #[cfg(test)]
    pub fn unlock(
        &mut self,
        path: &Path,
        password: Zeroizing<String>,
        create: bool,
    ) -> Result<Status> {
        self.unlock_with_keyfile(path, password, create, None)
    }
    pub fn unlock_with_keyfile(
        &mut self,
        path: &Path,
        password: Zeroizing<String>,
        create: bool,
        key_file: Option<&[u8]>,
    ) -> Result<Status> {
        if self.vault.is_some() {
            bail!("Lock the current vault first");
        }
        if create && password.chars().count() < 12 {
            bail!("Use at least 12 characters for the master password");
        }
        let absolute = if create {
            path.parent()
                .context("Choose a vault folder")?
                .canonicalize()?
                .join(path.file_name().context("Choose a vault filename")?)
        } else {
            path.canonicalize()?
        };
        let mut key = DatabaseKey::new().with_password(&password);
        if let Some(bytes) = key_file {
            key = key.with_keyfile(&mut &bytes[..])?;
        }
        let mut vault = if create {
            let mut db = Database::new();
            db.root_mut().name = "Harbor".into();
            db.config.kdf_config = KdfConfig::Argon2id {
                iterations: 3,
                memory: 64 * 1024 * 1024,
                parallelism: 2,
                version: argon2::Version::Version13,
            };
            Vault {
                db,
                key,
                path: absolute,
                digest: vec![],
                uses_key_file: key_file.is_some(),
            }
        } else {
            let bytes = fs::read(&absolute).context("Could not read the vault")?;
            let db = Database::open(&mut bytes.as_slice(), key.clone())
                .map_err(|_| anyhow::anyhow!("Could not unlock vault. Check the master password and, if required, Windows Hello or the recovery key file."))?;
            Vault {
                db,
                key,
                path: absolute,
                digest: digest(&bytes),
                uses_key_file: key_file.is_some(),
            }
        };
        if create {
            vault.write(vault.db.clone(), true)?;
        }
        self.sql.execute(
            "INSERT OR REPLACE INTO settings(key,value) VALUES('vault',?)",
            [vault.path.to_string_lossy().as_ref()],
        )?;
        // Rebuild the local index from Harbor's custom fields, so a copied KDBX is portable.
        self.import_profiles(&vault)?;
        self.vault = Some(vault);
        self.status()
    }

    fn import_profiles(&mut self, vault: &Vault) -> Result<()> {
        let tx = self.sql.transaction()?;
        let path = vault.path.to_string_lossy();
        tx.execute("DELETE FROM profiles WHERE vault=?", [path.as_ref()])?;
        fn visit(
            group: keepass::db::GroupRef<'_>,
            tx: &rusqlite::Transaction<'_>,
            path: &str,
        ) -> Result<()> {
            for e in group.entries() {
                if e.get("Harbor.Type") != Some("ssh") {
                    continue;
                }
                let id = e.id().uuid().to_string();
                let forwards: Vec<Rule> =
                    serde_json::from_str(e.get("Harbor.Forwards").unwrap_or("[]"))
                        .context("Invalid forwarding settings in vault")?;
                forwarding::validate(&forwards)?;
                tx.execute(
                    "INSERT OR REPLACE INTO profiles(id,vault,entry,name,host,port,folder,forwards,tags) VALUES(?,?,?,?,?,?,?,?,?)",
                    params![
                        format!("{}:{}", path, id),
                        path,
                        id,
                        e.get_title().unwrap_or("Untitled"),
                        e.get("Harbor.Host").unwrap_or(""),
                        e.get("Harbor.Port")
                            .and_then(|p| p.parse::<u16>().ok())
                            .unwrap_or(22),
                        e.get("Harbor.Folder").unwrap_or("General"),
                        serde_json::to_string(&forwards)?,
                        serde_json::to_string(&e.tags)?
                    ],
                )?;
            }
            for child in group.groups() {
                visit(child, tx, path)?;
            }
            Ok(())
        }
        visit(vault.db.root(), &tx, path.as_ref())?;
        tx.commit()?;
        Ok(())
    }

    fn entry_id(&self, id: &str) -> Result<EntryId> {
        let v = self.vault.as_ref().context("Unlock your vault first")?;
        let raw: String = self
            .sql
            .query_row(
                "SELECT entry FROM profiles WHERE id=? AND vault=?",
                params![id, v.path.to_string_lossy().as_ref()],
                |r| r.get(0),
            )
            .context("Connection not found in this vault")?;
        Ok(EntryId::from_uuid(Uuid::parse_str(&raw)?))
    }

    pub fn username(&self, id: &str) -> Result<String> {
        let entry_id = self.entry_id(id)?;
        let e = self
            .vault
            .as_ref()
            .context("Vault is locked")?
            .db
            .entry(entry_id)
            .context("Credential entry is missing")?;
        Ok(e.get_username().unwrap_or("").into())
    }

    pub fn save_profile(&mut self, mut p: SaveProfile) -> Result<Status> {
        let secret = p.password.take().map(Zeroizing::new);
        let username = Zeroizing::new(std::mem::take(&mut p.username));
        forwarding::validate(&p.forwards)?;
        let forwards_json = serde_json::to_string(&p.forwards)?;
        let tags = normalize_tags(&p.tags)?;
        let tags_json = serde_json::to_string(&tags)?;
        let name = p.name.trim();
        let host = p.host.trim();
        if name.is_empty() || host.is_empty() || username.is_empty() || p.port == 0 {
            bail!("Name, host, username, and a valid port are required");
        }
        if host.chars().any(|c| c.is_whitespace() || c.is_control()) || host.contains('/') {
            bail!("Enter a hostname or IP address without a protocol or path");
        }
        if let Some(id) = &p.startup_snippet {
            self.snippet(id)?;
        }
        let existing = p.id.as_deref().map(|id| self.entry_id(id)).transpose()?;
        let vault = self.vault.as_mut().context("Unlock your vault first")?;
        let mut candidate = vault.db.clone();
        let new_id = if existing.is_none() {
            Some(candidate.root_mut().add_entry().id())
        } else {
            None
        };
        let entry_id = {
            let mut raw_entry = candidate
                .entry_mut(existing.or(new_id).context("Entry creation failed")?)
                .context("Credential entry is missing")?;
            let mut entry = raw_entry.track_changes();
            let method = auth::update(&mut entry.as_mut(), p.authentication.take())?;
            let entry_id = entry.id();
            entry.times.last_modification = Some(keepass::db::Times::now());
            entry.set_unprotected(fields::TITLE, name);
            entry.set_protected(fields::USERNAME, username.as_str());
            if method == auth::Method::Password {
                if let Some(password) = secret {
                    entry.set_protected(fields::PASSWORD, password.as_str());
                } else if existing.is_none() || entry.get_password().is_none() {
                    bail!("A password is required for password authentication");
                }
            }
            entry.set_unprotected(fields::URL, format!("ssh://{}:{}", host, p.port));
            entry.tags = tags;
            entry.set_unprotected("Harbor.Type", "ssh");
            entry.set_unprotected(
                "Harbor.StartupSnippet",
                p.startup_snippet.as_deref().unwrap_or(""),
            );
            entry.set_unprotected("Harbor.Host", host);
            entry.set_unprotected("Harbor.Port", p.port.to_string());
            entry.set_unprotected("Harbor.Forwards", forwards_json.clone());
            entry.set_unprotected(
                "Harbor.Folder",
                if p.folder.trim().is_empty() {
                    "General"
                } else {
                    p.folder.trim()
                },
            );
            entry_id
        };
        vault.write(candidate, false)?;
        let id =
            p.id.unwrap_or_else(|| format!("{}:{}", vault.path.to_string_lossy(), entry_id.uuid()));
        self.sql.execute(
            "INSERT OR REPLACE INTO profiles(id,vault,entry,name,host,port,folder,forwards,tags) VALUES(?,?,?,?,?,?,?,?,?)",
            params![
                id,
                vault.path.to_string_lossy().as_ref(),
                entry_id.uuid().to_string(),
                name,
                host,
                p.port,
                if p.folder.trim().is_empty() {
                    "General"
                } else {
                    p.folder.trim()
                },
                forwards_json,
                tags_json
            ],
        )?;
        self.status()
    }

    pub fn delete_profile(&mut self, id: &str) -> Result<Status> {
        let entry_id = self.entry_id(id)?;
        let vault = self.vault.as_mut().context("Unlock your vault first")?;
        let mut candidate = vault.db.clone();
        candidate
            .entry_mut(entry_id)
            .context("Credential entry is missing")?
            .track_changes()
            .remove();
        vault.write(candidate, false)?;
        self.sql.execute("DELETE FROM profiles WHERE id=?", [id])?;
        self.status()
    }

    pub fn ssh_credentials(
        &self,
        id: &str,
    ) -> Result<(Profile, Zeroizing<String>, auth::Material)> {
        let entry_id = self.entry_id(id)?;
        let e = self
            .vault
            .as_ref()
            .context("Unlock your vault first")?
            .db
            .entry(entry_id)
            .context("Credential entry missing")?;
        let p = self
            .status()?
            .profiles
            .into_iter()
            .find(|p| p.id == id)
            .context("Connection not found")?;
        Ok((
            p,
            Zeroizing::new(e.get_username().unwrap_or("").into()),
            auth::material(&e)?,
        ))
    }
    #[cfg(test)]
    pub fn credentials(&self, id: &str) -> Result<(Profile, Zeroizing<String>, Zeroizing<String>)> {
        let (profile, user, material) = self.ssh_credentials(id)?;
        let auth::Material::Password(password) = material else {
            bail!("Expected password credentials in this test");
        };
        Ok((profile, user, password))
    }

    pub fn lock(&mut self) {
        self.vault = None;
    }

    pub fn hello_record(&self, path: &str) -> Result<Option<hello::Envelope>> {
        let json = self
            .sql
            .query_row(
                "SELECT record FROM hello_records WHERE path=?",
                [path],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        json.map(|s| {
            serde_json::from_str(&s)
                .context("Invalid local Windows Hello record; use the recovery key file")
        })
        .transpose()
    }
    pub fn hello_material(
        &self,
        path: &Path,
        password: &str,
        recovery: &Path,
    ) -> Result<(String, Zeroizing<Vec<u8>>, bool)> {
        let v = self
            .vault
            .as_ref()
            .context("Unlock the vault before enabling Windows Hello")?;
        if path.canonicalize()? != v.path {
            bail!("The selected vault is not the unlocked vault");
        }
        let rekey = !v.uses_key_file;
        let bytes = if rekey {
            let mut bytes = Zeroizing::new(vec![0; 32]);
            getrandom::fill(&mut bytes)?;
            bytes
        } else {
            read_key_file(recovery)?
        };
        let mut key = DatabaseKey::new().with_password(password);
        if !rekey {
            key = key.with_keyfile(&mut bytes.as_slice())?;
        }
        Database::open(&mut fs::File::open(&v.path)?, key).map_err(|_| {
            anyhow::anyhow!("The master password or recovery key file is incorrect")
        })?;
        Ok((v.path.to_string_lossy().into_owned(), bytes, rekey))
    }
    pub fn commit_hello(
        &mut self,
        path: &str,
        password: &str,
        key_file: &[u8],
        envelope: &hello::Envelope,
        rekey: bool,
    ) -> Result<Status> {
        let v = self
            .vault
            .as_mut()
            .context("Vault was locked during Hello setup")?;
        if v.path.to_string_lossy() != path || v.uses_key_file == rekey {
            bail!("Vault changed during Hello setup. Start again.");
        }
        if digest(&fs::read(&v.path)?) != v.digest {
            bail!("Vault changed outside Harbor. Lock and reopen it before setting up Hello.");
        }
        let old = self
            .sql
            .query_row(
                "SELECT record FROM hello_records WHERE path=?",
                [path],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        // Persist the encrypted factor before rekeying; recovery is already saved by the caller.
        self.sql.execute(
            "INSERT OR REPLACE INTO hello_records VALUES(?,?)",
            params![path, serde_json::to_string(envelope)?],
        )?;
        if rekey {
            let key = DatabaseKey::new()
                .with_password(password)
                .with_keyfile(&mut &key_file[..])?;
            if let Err(error) = v.write_key(v.db.clone(), key, false, true) {
                if let Some(old) = old {
                    self.sql.execute(
                        "INSERT OR REPLACE INTO hello_records VALUES(?,?)",
                        params![path, old],
                    )?;
                } else {
                    self.sql
                        .execute("DELETE FROM hello_records WHERE path=?", [path])?;
                }
                return Err(error);
            }
            v.uses_key_file = true;
        }
        self.status()
    }

    pub fn known_key(&self, host: &str, port: u16) -> Result<Option<String>> {
        Ok(self
            .sql
            .query_row(
                "SELECT fingerprint FROM host_keys WHERE host=? AND port=?",
                params![host.to_lowercase(), port],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn trust_key(&self, host: &str, port: u16, fingerprint: &str) -> Result<()> {
        if let Some(known) = self.known_key(host, port)? {
            if known != fingerprint {
                bail!("Host key changed. Connection blocked.");
            }
        }
        self.sql.execute(
            "INSERT OR IGNORE INTO host_keys VALUES(?,?,?)",
            params![host.to_lowercase(), port, fingerprint],
        )?;
        Ok(())
    }
}

pub fn read_key_file(path: &Path) -> Result<Zeroizing<Vec<u8>>> {
    use std::io::Read;
    let mut bytes = Zeroizing::new(Vec::new());
    fs::File::open(path)
        .context("Could not open the recovery key file")?
        .take(65537)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > 65536 {
        bail!("Choose a nonempty key file smaller than 64 KiB");
    }
    Ok(bytes)
}
pub fn save_recovery(path: &Path, vault: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Choose a recovery file folder")?
        .canonicalize()?;
    let resolved = parent.join(path.file_name().context("Choose a recovery filename")?);
    if resolved == vault || resolved == vault.with_extension("kdbx.bak") {
        bail!("Keep the recovery key file separate from your vault and backup");
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(resolved).map_err(|e|e.error).context("Could not save recovery key file. Choose a new filename; existing files are never overwritten.")?;
    Ok(())
}

fn normalize_tags(input: &[String]) -> Result<Vec<String>> {
    let mut tags = Vec::new();
    for raw in input {
        let tag = raw.trim().to_lowercase();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 40 || tag.chars().any(|c| c.is_control() || c == ',' || c == ';') {
            bail!("Tags must be at most 40 characters without commas, semicolons, or control characters");
        }
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    if tags.len() > 20 {
        bail!("Use at most 20 tags per host");
    }
    tags.sort();
    Ok(tags)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: String,
    pub name: String,
    pub group: String,
    pub tags: Vec<String>,
    pub body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSnippet {
    pub id: Option<String>,
    pub name: String,
    pub group: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub body: String,
}

impl Storage {
    pub fn snippets(&self) -> Result<Vec<Snippet>> {
        let vault = self.vault.as_ref().context("Unlock your vault first")?;
        fn visit(group: keepass::db::GroupRef<'_>, snippets: &mut Vec<Snippet>) {
            for entry in group.entries() {
                if entry.get("Harbor.Type") == Some("snippet") {
                    snippets.push(Snippet {
                        id: entry.id().uuid().to_string(),
                        name: entry.get_title().unwrap_or("Untitled").into(),
                        group: entry.get("Harbor.SnippetGroup").unwrap_or("General").into(),
                        tags: entry.tags.clone(),
                        body: entry.get("Harbor.SnippetBody").unwrap_or("").into(),
                    });
                }
            }
            for child in group.groups() {
                visit(child, snippets);
            }
        }
        let mut snippets = Vec::new();
        visit(vault.db.root(), &mut snippets);
        snippets.sort_by(|a, b| (&a.group, &a.name, &a.id).cmp(&(&b.group, &b.name, &b.id)));
        Ok(snippets)
    }

    pub fn snippet(&self, id: &str) -> Result<Snippet> {
        self.snippets()?
            .into_iter()
            .find(|s| s.id == id)
            .context("Saved snippet no longer exists; update the host's startup snippet")
    }

    pub fn startup_command(&self, profile: &Profile) -> Result<Option<Zeroizing<String>>> {
        profile
            .startup_snippet
            .as_deref()
            .map(|id| {
                let snippet = self.snippet(id)?;
                validate_snippet_body(&snippet.body)?;
                Ok(Zeroizing::new(format!(
                    "{}\r",
                    snippet
                        .body
                        .replace("\r\n", "\n")
                        .trim_end_matches(['\n', '\r'])
                )))
            })
            .transpose()
    }

    pub fn save_snippet(&mut self, input: SaveSnippet) -> Result<Vec<Snippet>> {
        let name = input.name.trim();
        let group = input.group.trim();
        anyhow::ensure!(
            !name.is_empty() && name.chars().count() <= 120,
            "Snippet name is required and must be at most 120 characters"
        );
        anyhow::ensure!(
            group.chars().count() <= 80,
            "Snippet group must be at most 80 characters"
        );
        validate_snippet_body(&input.body)?;
        let tags = normalize_tags(&input.tags)?;
        let existing = input
            .id
            .as_deref()
            .map(|id| {
                self.snippet(id)?;
                Ok::<_, anyhow::Error>(EntryId::from_uuid(Uuid::parse_str(id)?))
            })
            .transpose()?;
        let vault = self.vault.as_mut().context("Unlock your vault first")?;
        let mut candidate = vault.db.clone();
        let id = existing.unwrap_or_else(|| candidate.root_mut().add_entry().id());
        {
            let mut raw = candidate.entry_mut(id).context("Snippet missing")?;
            let mut entry = raw.track_changes();
            entry.times.last_modification = Some(keepass::db::Times::now());
            entry.set_unprotected(fields::TITLE, name);
            entry.set_unprotected("Harbor.Type", "snippet");
            entry.set_unprotected(
                "Harbor.SnippetGroup",
                if group.is_empty() { "General" } else { group },
            );
            entry.set_protected("Harbor.SnippetBody", input.body.replace("\r\n", "\n"));
            entry.tags = tags;
        }
        vault.write(candidate, false)?;
        self.snippets()
    }

    pub fn delete_snippet(&mut self, id: &str) -> Result<Vec<Snippet>> {
        self.snippet(id)?;
        if let Some(host) = self
            .status()?
            .profiles
            .iter()
            .find(|p| p.startup_snippet.as_deref() == Some(id))
        {
            bail!("This snippet is used at startup by '{}'. Remove it from that host before deleting it.", host.name);
        }
        let vault = self.vault.as_mut().context("Unlock your vault first")?;
        let mut candidate = vault.db.clone();
        candidate
            .entry_mut(EntryId::from_uuid(Uuid::parse_str(id)?))
            .context("Snippet missing")?
            .track_changes()
            .remove();
        vault.write(candidate, false)?;
        self.snippets()
    }
}

fn validate_snippet_body(body: &str) -> Result<()> {
    anyhow::ensure!(
        !body.trim().is_empty() && body.len() <= 65536,
        "Snippet commands are required and must be at most 64 KiB"
    );
    anyhow::ensure!(
        !body
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t'),
        "Snippet commands cannot contain terminal control characters"
    );
    Ok(())
}
