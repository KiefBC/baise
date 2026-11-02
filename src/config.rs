//! `mod-status.toml`, the file a mod repo carries: where the game's version
//! comes from, how the cards look, and one `[[mod]]` table per mod.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::clock::is_date;
use crate::state::Status;

/// The default `version_regex`: Steam announcement titles like
/// "Patch Notes Version 2.04.05". Capture group 1 is the version.
pub const DEFAULT_VERSION_REGEX: &str = r"(?i)patch notes.*?version\s*(\d+(?:\.\d+)+)";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub source: SourceConfig,
    #[serde(default)]
    pub render: RenderConfig,
    #[serde(rename = "mod", default)]
    pub mods: Vec<ModDecl>,
}

/// Where the game's current build comes from.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum SourceConfig {
    /// api.steamcmd.net for the public branch's build id, ISteamNews for the
    /// human-readable patch version.
    Steam {
        app_id: u32,
        #[serde(default)]
        version_regex: Option<String>,
        /// Steam branch to read; almost always `public`.
        #[serde(default)]
        branch: Option<String>,
    },
    /// A build written into the config by hand, for games that are not on
    /// Steam: edit it when the game updates.
    Manual {
        build: String,
        #[serde(default)]
        version: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderConfig {
    #[serde(default = "default_layout")]
    pub layout: String,
    /// A built-in theme name (`nexus`, `crimson-desert`) or a path to a theme
    /// `.toml`, relative to the config file.
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_title_prefix")]
    pub title_prefix: String,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            layout: default_layout(),
            theme: default_theme(),
            title_prefix: default_title_prefix(),
        }
    }
}

fn default_layout() -> String {
    "card".into()
}
fn default_theme() -> String {
    "nexus".into()
}
fn default_title_prefix() -> String {
    "LIVE STATUS // AUTO-CHECKED".into()
}

/// One mod: what it was verified on and what to show either way.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModDecl {
    /// File stem of every output: `<key>.gif`, `<key>.png`, `<key>.json`.
    pub key: String,
    pub label: String,
    /// The game build this mod was verified on.
    pub build: String,
    /// When it was verified, `YYYY-MM-DD`.
    pub date: String,
    /// What to show while the game is still on `build`.
    #[serde(default = "default_status")]
    pub status: Status,
    /// What to show once the game has moved past `build`.
    #[serde(default = "default_on_update")]
    pub on_update: Status,
    #[serde(default)]
    pub note: String,
}

fn default_status() -> Status {
    Status::Working
}
fn default_on_update() -> Status {
    Status::Untested
}

impl Config {
    pub fn parse(text: &str) -> Result<Self> {
        let cfg: Config = toml::from_str(text).context("mod-status.toml does not parse")?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("in {}", path.display()))
    }

    pub fn find(&self, key: &str) -> Option<&ModDecl> {
        self.mods.iter().find(|m| m.key == key)
    }

    fn validate(&self) -> Result<()> {
        if self.mods.is_empty() {
            bail!("no [[mod]] tables: declare at least one mod");
        }
        if self.render.layout != "card" {
            bail!(
                "render.layout = {:?}: the only layout is \"card\"",
                self.render.layout
            );
        }
        match &self.source {
            SourceConfig::Steam {
                version_regex: Some(re),
                ..
            } => {
                let compiled = regex::Regex::new(re)
                    .with_context(|| format!("source.version_regex {re:?} is not a regex"))?;
                if compiled.captures_len() < 2 {
                    bail!("source.version_regex needs a capture group for the version");
                }
            }
            SourceConfig::Manual { build, .. } if build.trim().is_empty() => {
                bail!("source.build is empty");
            }
            _ => {}
        }
        let mut seen = BTreeSet::new();
        for m in &self.mods {
            if !valid_key(&m.key) {
                bail!(
                    "mod key {:?}: use lowercase letters, digits, '-' and '_' (it is a file name)",
                    m.key
                );
            }
            if !seen.insert(m.key.as_str()) {
                bail!("mod key {:?} is declared twice", m.key);
            }
            if m.build.trim().is_empty() {
                bail!("mod {:?}: build is empty", m.key);
            }
            if !is_date(&m.date) {
                bail!("mod {:?}: date {:?} is not YYYY-MM-DD", m.key, m.date);
            }
            if !m.status.declarable() {
                bail!(
                    "mod {:?}: status = {:?}; use working, broken or wip",
                    m.key,
                    m.status.as_str()
                );
            }
            if !m.on_update.on_update_allowed() {
                bail!(
                    "mod {:?}: on_update = {:?}; use untested or broken",
                    m.key,
                    m.on_update.as_str()
                );
            }
        }
        Ok(())
    }
}

/// Keys become file names and URL path segments.
pub fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && !key.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"
[source]
kind = "steam"
app_id = 3321460

[render]
theme = "crimson-desert"

[[mod]]
key = "desert-tooling"
label = "Desert Tooling"
build = "26361232"
date = "2026-11-01"
"#;

    #[test]
    fn example_parses_with_defaults() {
        let cfg = Config::parse(EXAMPLE).unwrap();
        assert!(matches!(cfg.source, SourceConfig::Steam { app_id: 3321460, .. }));
        assert_eq!(cfg.render.layout, "card");
        assert_eq!(cfg.render.title_prefix, "LIVE STATUS // AUTO-CHECKED");
        let m = cfg.find("desert-tooling").unwrap();
        assert_eq!(m.status, Status::Working);
        assert_eq!(m.on_update, Status::Untested);
    }

    #[test]
    fn manual_source() {
        let text = EXAMPLE.replace(
            "kind = \"steam\"\napp_id = 3321460",
            "kind = \"manual\"\nbuild = \"1.2\"",
        );
        let cfg = Config::parse(&text).unwrap();
        assert!(matches!(cfg.source, SourceConfig::Manual { ref build, .. } if build == "1.2"));
    }

    #[test]
    fn rejects_bad_declarations() {
        let cases = [
            ("key = \"desert-tooling\"", "key = \"Desert Tooling\""),
            ("date = \"2026-11-01\"", "date = \"yesterday\""),
            ("build = \"26361232\"", "build = \"\""),
            (
                "date = \"2026-11-01\"",
                "date = \"2026-11-01\"\nstatus = \"untested\"",
            ),
            (
                "date = \"2026-11-01\"",
                "date = \"2026-11-01\"\non_update = \"working\"",
            ),
            ("date = \"2026-11-01\"", "date = \"2026-11-01\"\ncolour = \"red\""),
            ("theme = \"crimson-desert\"", "layout = \"strip\""),
            (
                "app_id = 3321460",
                "app_id = 3321460\nversion_regex = \"no group\"",
            ),
        ];
        for (from, to) in cases {
            let text = EXAMPLE.replace(from, to);
            assert!(Config::parse(&text).is_err(), "should reject: {to}");
        }
        let twice = format!("{EXAMPLE}\n[[mod]]\nkey = \"desert-tooling\"\nlabel = \"x\"\nbuild = \"1\"\ndate = \"2026-01-01\"\n");
        assert!(Config::parse(&twice).is_err());
        let none = "[source]\nkind = \"manual\"\nbuild = \"1\"\n";
        assert!(Config::parse(none).is_err());
    }

    #[test]
    fn default_regex_has_a_group() {
        assert_eq!(
            regex::Regex::new(DEFAULT_VERSION_REGEX).unwrap().captures_len(),
            2
        );
    }
}
