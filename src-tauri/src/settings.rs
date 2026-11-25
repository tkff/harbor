use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub version: u8,
    pub connection_logging: bool,
    pub session_logging: bool,
    pub theme: String,
    pub accent: String,
    pub density: String,
    pub reduced_motion: bool,
    pub font_family: String,
    pub font_size: u16,
    pub line_height: f64,
    pub terminal_theme: String,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub scrollback: u32,
    pub default_port: u16,
    pub default_group: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            connection_logging: true,
            session_logging: false,
            theme: "system".into(),
            accent: "teal".into(),
            density: "comfortable".into(),
            reduced_motion: false,
            font_family: "system".into(),
            font_size: 14,
            line_height: 1.35,
            terminal_theme: "harbor".into(),
            cursor_style: "bar".into(),
            cursor_blink: true,
            scrollback: 5000,
            default_port: 22,
            default_group: "General".into(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(self.version == 1, "Unsupported settings version");
        for (name, value, allowed) in [
            (
                "theme",
                self.theme.as_str(),
                &["system", "light", "dark"][..],
            ),
            (
                "accent",
                self.accent.as_str(),
                &["teal", "blue", "violet"][..],
            ),
            (
                "density",
                self.density.as_str(),
                &["comfortable", "compact"][..],
            ),
            (
                "font family",
                self.font_family.as_str(),
                &["system", "cascadia", "consolas", "jetbrains"][..],
            ),
            (
                "terminal theme",
                self.terminal_theme.as_str(),
                &["harbor", "light", "contrast"][..],
            ),
            (
                "cursor style",
                self.cursor_style.as_str(),
                &["bar", "block", "underline"][..],
            ),
        ] {
            anyhow::ensure!(allowed.contains(&value), "Invalid {name}");
        }
        anyhow::ensure!(
            (10..=28).contains(&self.font_size),
            "Font size must be between 10 and 28"
        );
        anyhow::ensure!(
            self.line_height.is_finite() && (1.0..=2.0).contains(&self.line_height),
            "Line height must be between 1 and 2"
        );
        anyhow::ensure!(
            (1000..=50000).contains(&self.scrollback),
            "Scrollback must be between 1,000 and 50,000 lines"
        );
        anyhow::ensure!(
            self.default_port > 0,
            "Default SSH port must be between 1 and 65535"
        );
        anyhow::ensure!(
            !self.default_group.trim().is_empty()
                && self.default_group.chars().count() <= 80
                && !self.default_group.chars().any(char::is_control),
            "Default group must contain 1 to 80 characters without control characters"
        );
        Ok(())
    }
}
pub fn load(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    anyhow::ensure!(
        fs::metadata(path)?.len() <= 65536,
        "Settings file is too large"
    );
    let settings: Settings = serde_json::from_slice(&fs::read(path)?)
        .context("Settings file could not be read; open Settings to restore defaults")?;
    settings.validate()?;
    Ok(settings)
}
pub fn save(path: &Path, mut settings: Settings) -> Result<Settings> {
    settings.default_group = settings.default_group.trim().into();
    settings.validate()?;
    let parent = path.parent().context("Settings directory missing")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec_pretty(&settings)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(settings)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_roundtrip_migration_and_invalid_writes_preserve_saved_settings() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("settings.json");
        assert_eq!(load(&path)?, Settings::default());
        assert!(load(&path)?.connection_logging);
        assert!(!load(&path)?.session_logging);
        fs::write(&path, br#"{"version":1,"theme":"dark"}"#)?;
        let mut settings = load(&path)?;
        assert_eq!(settings.theme, "dark");
        assert_eq!(settings.font_size, 14);
        settings.default_group = " Work ".into();
        settings.font_size = 18;
        let saved = save(&path, settings)?;
        assert_eq!(saved.default_group, "Work");
        assert_eq!(load(&path)?, saved);
        let bytes = fs::read(&path)?;
        for invalid in [
            Settings {
                theme: "unknown".into(),
                ..saved.clone()
            },
            Settings {
                font_size: 0,
                ..saved.clone()
            },
            Settings {
                scrollback: 500001,
                ..saved.clone()
            },
            Settings {
                default_port: 0,
                ..saved.clone()
            },
            Settings {
                line_height: f64::NAN,
                ..saved.clone()
            },
            Settings {
                version: 2,
                ..saved.clone()
            },
        ] {
            assert!(save(&path, invalid).is_err());
            assert_eq!(fs::read(&path)?, bytes);
        }
        fs::write(&path, b"broken json")?;
        assert!(load(&path).is_err());
        save(&path, Settings::default())?;
        assert_eq!(load(&path)?, Settings::default());
        Ok(())
    }
}
