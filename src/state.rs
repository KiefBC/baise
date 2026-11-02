//! The rule, and `state.json`, the only memory the kit has between runs.
//!
//! `state.json` lives on the `badges` branch next to the images, never on the
//! consumer's main branch, so their history never sees a bot commit.
//!
//! ```text
//! first_seen[current.build] ||= today
//! for each mod:
//!   if overrides[mod].build == current.build -> override status, override date
//!   elif mod.build == current.build          -> mod.status, mod.date
//!   else                                     -> mod.on_update, first_seen[current.build]
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::clock::Timestamp;
use crate::config::ModDecl;
use crate::source::GameVersion;

/// What a card says about a mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Working,
    Untested,
    Wip,
    Broken,
}

impl Status {
    pub const ALL: [Status; 4] = [Status::Working, Status::Untested, Status::Wip, Status::Broken];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Working => "working",
            Status::Untested => "untested",
            Status::Wip => "wip",
            Status::Broken => "broken",
        }
    }

    /// The big pulsing line on the card.
    pub fn headline(self) -> &'static str {
        match self {
            Status::Working => "WORKING",
            Status::Untested => "GAME UPDATED - UNTESTED",
            Status::Wip => "IN DEVELOPMENT",
            Status::Broken => "BROKEN - FIX IN PROGRESS",
        }
    }

    /// The label over the card's middle column (the date column).
    pub fn since_label(self) -> &'static str {
        match self {
            Status::Working => "VERIFIED",
            Status::Untested => "PATCH DETECTED",
            Status::Wip => "LAST UPDATE",
            Status::Broken => "BROKEN SINCE",
        }
    }

    /// The short word on the shields.io badge.
    pub fn short(self) -> &'static str {
        match self {
            Status::Working => "working",
            Status::Untested => "untested",
            Status::Wip => "in development",
            Status::Broken => "broken",
        }
    }

    /// May a mod declare this as its `status` (what to show while the build
    /// matches)? "untested" only ever comes from the game moving.
    pub fn declarable(self) -> bool {
        !matches!(self, Status::Untested)
    }

    /// May a mod declare this as its `on_update`?
    pub fn on_update_allowed(self) -> bool {
        matches!(self, Status::Untested | Status::Broken)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Status {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        Status::ALL
            .into_iter()
            .find(|st| st.as_str() == s)
            .ok_or_else(|| anyhow!("unknown status {s:?}: use working, untested, wip or broken"))
    }
}

/// A manual status set with `baise set`, valid only on the build it was
/// set on: the next game update falls back to the rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Override {
    pub status: Status,
    pub build: String,
    pub date: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Game build -> the date this kit first saw it. The "PATCH DETECTED" date.
    #[serde(default)]
    pub first_seen: BTreeMap<String, String>,
    /// Mod key -> manual override.
    #[serde(default)]
    pub overrides: BTreeMap<String, Override>,
    #[serde(default)]
    pub latest_build: Option<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
    /// When the source says the build went live (Steam's `timeupdated`).
    #[serde(default)]
    pub latest_updated: Option<i64>,
    /// The SYNC counter on the card: the workflow passes its run number.
    #[serde(default)]
    pub sync: u64,
    /// When the source was last read successfully, RFC 3339.
    #[serde(default)]
    pub last_sync: Option<String>,
}

/// Where a resolved status came from, for the CLI's report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Declared,
    GameMoved,
    Override,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub key: String,
    pub label: String,
    pub status: Status,
    /// The date for the middle column.
    pub since: String,
    pub note: String,
    pub origin: Origin,
}

impl State {
    /// Reads `state.json`; a missing file is a first run, not an error.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("{} is not a valid state file", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        std::fs::write(path, text).with_context(|| format!("cannot write {}", path.display()))
    }

    /// Records a successful read of the source.
    pub fn observe(&mut self, current: &GameVersion, now: Timestamp, sync: Option<u64>) {
        self.first_seen
            .entry(current.build.clone())
            .or_insert_with(|| now.date());
        // A news outage (no version) keeps the version already known for the
        // same build; a new build without a version shows none rather than
        // the old build's.
        let same_build = self.latest_build.as_deref() == Some(current.build.as_str());
        if current.version.is_some() || !same_build {
            self.latest_version.clone_from(&current.version);
        }
        self.latest_build = Some(current.build.clone());
        self.latest_updated = current.updated.map(|t| t.0);
        self.sync = sync.unwrap_or_else(|| self.sync.saturating_add(1));
        self.last_sync = Some(now.rfc3339());
    }

    /// The game as last observed, for re-rendering without the network.
    pub fn current(&self) -> Result<GameVersion> {
        let build = self
            .latest_build
            .clone()
            .ok_or_else(|| anyhow!("state has no build yet: run `baise check` first"))?;
        Ok(GameVersion {
            build,
            version: self.latest_version.clone(),
            updated: self.latest_updated.map(Timestamp),
        })
    }

    pub fn last_sync(&self) -> Option<Timestamp> {
        self.last_sync.as_deref().and_then(Timestamp::parse_rfc3339)
    }

    /// `baise set`: pin `status` for `key` on `build`; `None` clears it.
    pub fn set_override(
        &mut self,
        key: &str,
        status: Option<Status>,
        build: Option<&str>,
        now: Timestamp,
    ) -> Result<()> {
        let Some(status) = status else {
            self.overrides.remove(key);
            return Ok(());
        };
        let build = match build.or(self.latest_build.as_deref()) {
            Some(b) => b.to_owned(),
            None => bail!("no build to pin the override to: pass --build or run `check` first"),
        };
        self.overrides.insert(
            key.to_owned(),
            Override {
                status,
                build,
                date: now.date(),
            },
        );
        Ok(())
    }
}

/// The rule. Pure: the same declaration, build and state always give the same answer.
pub fn resolve(decl: &ModDecl, current_build: &str, state: &State) -> Resolved {
    let (status, since, origin) = match state.overrides.get(&decl.key) {
        Some(o) if o.build == current_build => (o.status, o.date.clone(), Origin::Override),
        _ if decl.build == current_build => (decl.status, decl.date.clone(), Origin::Declared),
        _ => {
            let since = state
                .first_seen
                .get(current_build)
                .cloned()
                .unwrap_or_else(|| decl.date.clone());
            (decl.on_update, since, Origin::GameMoved)
        }
    };
    Resolved {
        key: decl.key.clone(),
        label: decl.label.clone(),
        status,
        since,
        note: decl.note.clone(),
        origin,
    }
}
