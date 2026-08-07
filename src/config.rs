use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub tmdb_key: String,
    pub radarr: Radarr,
    /// TMDB person id -> roles to follow. `actor` matches cast credits, any
    /// other value is matched as a substring against the crew `job` field.
    /// TOML keys are strings, so ids are parsed in `Config::people`.
    pub people: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub notifications: Notifications,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Radarr {
    pub host: String,
    pub port: u16,
    pub key: String,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub urlbase: Option<String>,
    /// Only consider credits released within this many days. Omit for no
    /// release-date filter.
    pub daysreleased: Option<i64>,
    pub root_folder_path: String,
    #[serde(default = "default_true")]
    pub monitored: bool,
    #[serde(default = "default_true")]
    pub search: bool,
    pub minimum_availability: Option<String>,
    /// Original-language code -> quality profile id. Must contain `default`.
    pub profiles: BTreeMap<String, u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notifications {
    pub homeassistant: Option<HomeAssistant>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HomeAssistant {
    pub url: String,
    #[serde(default = "default_title")]
    pub title: String,
}

fn default_true() -> bool {
    true
}

fn default_title() -> String {
    "Radarr-Auto-Add".to_string()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?;
        Self::from_str(&raw).with_context(|| format!("in config file {}", path.display()))
    }

    pub fn from_str(raw: &str) -> Result<Self> {
        let config: Config = toml::from_str(raw)?;
        if !config.radarr.profiles.contains_key("default") {
            anyhow::bail!("radarr.profiles must contain a `default` entry");
        }
        Ok(config)
    }

    /// The `[people]` table with its keys parsed as TMDB person ids.
    pub fn people(&self) -> Result<Vec<(u64, &[String])>> {
        self.people
            .iter()
            .map(|(id, roles)| {
                let id = id
                    .parse::<u64>()
                    .with_context(|| format!("`{id}` in [people] is not a valid TMDB person id"))?;
                Ok((id, roles.as_slice()))
            })
            .collect()
    }

    pub fn quality_profile_for(&self, language: Option<&str>) -> u32 {
        language
            .and_then(|lang| self.radarr.profiles.get(lang))
            .or_else(|| self.radarr.profiles.get("default"))
            .copied()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> Config {
        Config::load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/config.example.toml"
        )))
        .expect("config.example.toml should be valid")
    }

    #[test]
    fn example_config_parses() {
        let config = example();
        assert_eq!(config.radarr.port, 7878);
        assert_eq!(config.radarr.root_folder_path, "/movies");
        assert!(config.notifications.homeassistant.is_some());
    }

    #[test]
    fn people_keys_parse_as_ids() {
        let config = example();
        let people = config.people().unwrap();
        assert_eq!(people[0].0, 287);
        assert_eq!(people[0].1, ["actor"]);
        assert_eq!(people[1].1, ["director", "writer"]);
    }

    #[test]
    fn bad_person_id_is_rejected() {
        let mut config = example();
        config.people.insert("brad pitt".into(), vec![]);
        let err = config.people().unwrap_err().to_string();
        assert!(err.contains("brad pitt"), "unexpected error: {err}");
    }

    #[test]
    fn profiles_fall_back_to_default() {
        let config = example();
        assert_eq!(config.quality_profile_for(Some("does-not-exist")), 1);
        assert_eq!(config.quality_profile_for(None), 1);
    }

    const MINIMAL: &str = r#"
        tmdb_key = "k"
        [radarr]
        host = "http://radarr"
        port = 7878
        key = "k"
        root_folder_path = "/movies"
        [radarr.profiles]
        default = 1
        [people]
        287 = ["actor"]
    "#;

    #[test]
    fn optional_fields_default_sensibly() {
        let config = Config::from_str(MINIMAL).unwrap();
        assert!(config.radarr.monitored);
        assert!(config.radarr.search);
        assert!(config.radarr.daysreleased.is_none());
        assert!(config.radarr.minimum_availability.is_none());
        assert!(config.notifications.homeassistant.is_none());
    }

    #[test]
    fn missing_default_profile_is_rejected() {
        let toml = MINIMAL.replace("default = 1", "en = 1");
        let err = Config::from_str(&toml).unwrap_err().to_string();
        assert!(err.contains("default"), "unexpected error: {err}");
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let toml = format!("{MINIMAL}\n[notifications.pushbullet]\nkey = \"k\"\n");
        let err = Config::from_str(&toml).unwrap_err().to_string();
        assert!(err.contains("pushbullet"), "unexpected error: {err}");
    }
}
