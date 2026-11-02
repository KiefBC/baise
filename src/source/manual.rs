//! `[source] kind = "manual"`: the build is whatever the config says. Also what
//! `check --offline --build B` uses.

use anyhow::Result;

use super::{GameVersion, Source};

#[derive(Debug, Clone)]
pub struct Manual {
    pub build: String,
    pub version: Option<String>,
}

impl Source for Manual {
    fn current(&self) -> Result<GameVersion> {
        Ok(GameVersion {
            build: self.build.clone(),
            version: self.version.clone(),
            updated: None,
        })
    }
}
