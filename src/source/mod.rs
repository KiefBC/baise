//! The Source brick: what the game is on now.

pub mod manual;
pub mod steam;

use anyhow::Result;

use crate::clock::Timestamp;
use crate::config::{SourceConfig, DEFAULT_VERSION_REGEX};

/// The game as a source sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameVersion {
    /// The build id the mods declare against (Steam's `buildid`).
    pub build: String,
    /// The human-readable patch version, when the source knows one.
    pub version: Option<String>,
    /// When the build went live, when the source knows.
    pub updated: Option<Timestamp>,
}

pub trait Source {
    /// Reads the current version. An `Err` means "unreachable": the caller
    /// keeps the last good state instead of drawing a wrong one.
    fn current(&self) -> Result<GameVersion>;
}

/// Builds the source a config asks for.
pub fn from_config(cfg: &SourceConfig) -> Result<Box<dyn Source>> {
    Ok(match cfg {
        SourceConfig::Steam {
            app_id,
            version_regex,
            branch,
        } => Box::new(steam::Steam::new(
            *app_id,
            branch.clone().unwrap_or_else(|| "public".into()),
            version_regex.as_deref().unwrap_or(DEFAULT_VERSION_REGEX),
        )?),
        SourceConfig::Manual { build, version } => Box::new(manual::Manual {
            build: build.clone(),
            version: version.clone(),
        }),
    })
}
