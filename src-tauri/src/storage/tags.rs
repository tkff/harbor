use super::*;
use std::collections::BTreeSet;

fn entries(group: keepass::db::GroupRef<'_>, ids: &mut Vec<EntryId>) {
    ids.extend(group.entries().map(|e| e.id()));
    for child in group.groups() {
        entries(child, ids);
    }
}

fn names(db: &Database) -> Result<Vec<String>> {
    let mut ids = vec![];
    entries(db.root(), &mut ids);
    let mut tags = BTreeSet::new();
    for id in ids {
        let entry = db.entry(id).context("Tag entry missing")?;
        match entry.get("Harbor.Type") {
            Some("ssh") => tags.extend(entry.tags.iter().cloned()),
            Some("tagRegistry") => tags.extend(
                serde_json::from_str::<Vec<String>>(entry.get("Harbor.TagNames").unwrap_or("[]"))
                    .context("Invalid tag registry")?,
            ),
            _ => {}
        }
    }
    Ok(tags.into_iter().collect())
}

pub(super) fn preserve(db: &mut Database, additional: &[String]) -> Result<()> {
    let mut tags: BTreeSet<_> = names(db)?.into_iter().collect();
    tags.extend(additional.iter().cloned());
    write_registry(db, tags.into_iter().collect())
}

fn write_registry(db: &mut Database, tags: Vec<String>) -> Result<()> {
    let mut ids = vec![];
    entries(db.root(), &mut ids);
    let registries: Vec<_> = ids
        .into_iter()
        .filter(|id| {
            db.entry(*id)
                .is_some_and(|e| e.get("Harbor.Type") == Some("tagRegistry"))
        })
        .collect();
    let id = registries
        .first()
        .copied()
        .unwrap_or_else(|| db.root_mut().add_entry().id());
    for duplicate in registries.into_iter().skip(1) {
        db.entry_mut(duplicate).unwrap().track_changes().remove();
    }
    let mut raw = db.entry_mut(id).context("Tag registry missing")?;
    let mut entry = raw.track_changes();
    entry.set_unprotected(fields::TITLE, "Harbor tags");
    entry.set_unprotected("Harbor.Type", "tagRegistry");
    entry.set_unprotected("Harbor.TagNames", serde_json::to_string(&tags)?);
    Ok(())
}

impl Storage {
    pub fn tags(&self) -> Result<Vec<String>> {
        names(&self.vault.as_ref().context("Unlock your vault first")?.db)
    }

    pub fn save_tag(&mut self, old_name: Option<String>, name: String) -> Result<Vec<String>> {
        let normalized = normalize_tags(&[name])?;
        let name = normalized.first().context("Enter a tag name")?;
        self.change_tag(old_name.as_deref(), Some(name))
    }

    pub fn delete_tag(&mut self, name: String) -> Result<Vec<String>> {
        self.change_tag(Some(&name), None)
    }

    fn change_tag(
        &mut self,
        old_name: Option<&str>,
        new_name: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut tags = self.tags()?;
        if let Some(old) = old_name {
            anyhow::ensure!(
                tags.iter().any(|tag| tag == old),
                "Tag no longer exists. Refresh and try again."
            );
        }
        if let Some(new) = new_name {
            anyhow::ensure!(
                old_name == Some(new) || !tags.iter().any(|tag| tag == new),
                "A tag with this name already exists"
            );
        }
        tags.retain(|tag| Some(tag.as_str()) != old_name);
        if let Some(new) = new_name {
            tags.push(new.into());
        }
        tags.sort();
        let vault = self.vault.as_mut().context("Unlock your vault first")?;
        let mut candidate = vault.db.clone();
        if let Some(old) = old_name {
            let mut ids = vec![];
            entries(candidate.root(), &mut ids);
            for id in ids {
                let e = candidate.entry(id).unwrap();
                if e.get("Harbor.Type") != Some("ssh") || !e.tags.iter().any(|tag| tag == old) {
                    continue;
                }
                let mut assigned: BTreeSet<_> =
                    e.tags.iter().filter(|tag| *tag != old).cloned().collect();
                if let Some(new) = new_name {
                    assigned.insert(new.into());
                }
                let mut raw = candidate.entry_mut(id).unwrap();
                let mut entry = raw.track_changes();
                entry.tags = assigned.into_iter().collect();
                entry.times.last_modification = Some(keepass::db::Times::now());
            }
        }
        write_registry(&mut candidate, tags)?;
        vault.write(candidate, false)?;
        // Rebuild assignments in the local index only after the vault write succeeds.
        let tx = self.sql.transaction()?;
        let mut ids = vec![];
        entries(vault.db.root(), &mut ids);
        for id in ids {
            let e = vault.db.entry(id).unwrap();
            if e.get("Harbor.Type") == Some("ssh") {
                tx.execute(
                    "UPDATE profiles SET tags=? WHERE vault=? AND entry=?",
                    params![
                        serde_json::to_string(&e.tags)?,
                        vault.path.to_string_lossy().as_ref(),
                        id.uuid().to_string()
                    ],
                )?;
            }
        }
        tx.commit()?;
        self.tags()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_tags_update_assignments_persist_unused_and_travel_with_vault() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("tags.kdbx");
        let mut store = Storage::open(&dir.path().join("first.db"))?;
        assert!(store.tags().is_err());
        store.unlock(&path, Zeroizing::new("test-password".into()), true)?;
        for name in ["First", "Second"] {
            store.save_profile(serde_json::from_value(serde_json::json!({"id":null,"name":name,"host":"example.test","port":22,"folder":"General","username":"tester","password":"secret","tags":["prod","linux"]}))?)?;
        }
        store.save_tag(None, " Unused ".into())?;
        assert_eq!(store.tags()?, ["linux", "prod", "unused"]);
        assert!(store.save_tag(None, "PROD".into()).is_err());
        assert!(store.save_tag(Some("prod".into()), "linux".into()).is_err());
        assert!(store.save_tag(None, "bad;tag".into()).is_err());
        store.save_tag(Some("prod".into()), "Production".into())?;
        assert!(store
            .status()?
            .profiles
            .iter()
            .all(|p| p.tags == ["linux", "production"]));
        store.delete_tag("linux".into())?;
        assert!(store
            .status()?
            .profiles
            .iter()
            .all(|p| p.tags == ["production"]));
        let mut copied = Storage::open(&dir.path().join("second.db"))?;
        copied.unlock(&path, Zeroizing::new("test-password".into()), false)?;
        assert_eq!(copied.tags()?, ["production", "unused"]);
        assert_eq!(copied.status()?.profiles.len(), 2);
        for host in store.status()?.profiles {
            store.delete_profile(&host.id)?;
        }
        assert_eq!(store.tags()?, ["production", "unused"]);
        // An outdated reader must not overwrite a newer vault or its tag assignments.
        assert!(copied.delete_tag("production".into()).is_err());
        assert_eq!(copied.tags()?, ["production", "unused"]);
        copied.lock();
        copied.unlock(&path, Zeroizing::new("test-password".into()), false)?;
        assert!(copied.status()?.profiles.is_empty());
        assert_eq!(copied.tags()?, ["production", "unused"]);
        Ok(())
    }
}
