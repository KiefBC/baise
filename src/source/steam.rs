//! Steam, through two public JSON endpoints, no key needed:
//!
//! - `api.steamcmd.net/v1/info/<appid>`: a mirror of SteamCMD's `app_info`,
//!   `data.<appid>.depots.branches.<branch>.{buildid,timeupdated}`.
//! - `ISteamNews/GetNewsForApp`: announcement titles, from which a regex pulls
//!   the patch version ("Patch Notes Version 2.04.05").
//!
//! The build is required (no build, no answer); the version is best effort.
//! Parsing is split from fetching so it can be tested on recorded fixtures.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde_json::Value;

use super::{GameVersion, Source};
use crate::clock::Timestamp;

const USER_AGENT: &str = concat!(
    "baise/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/KiefBC/baise)"
);

pub struct Steam {
    app_id: u32,
    branch: String,
    version_re: Regex,
    agent: ureq::Agent,
}

impl Steam {
    pub fn new(app_id: u32, branch: String, version_regex: &str) -> Result<Self> {
        let version_re = Regex::new(version_regex).context("version_regex")?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(USER_AGENT)
            .build()
            .into();
        Ok(Self {
            app_id,
            branch,
            version_re,
            agent,
        })
    }

    pub fn info_url(app_id: u32) -> String {
        format!("https://api.steamcmd.net/v1/info/{app_id}")
    }

    pub fn news_url(app_id: u32) -> String {
        format!(
            "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid={app_id}&count=20&maxlength=1&feeds=steam_community_announcements"
        )
    }

    fn get(&self, url: &str) -> Result<String> {
        self.agent
            .get(url)
            .call()
            .with_context(|| format!("GET {url}"))?
            .body_mut()
            .read_to_string()
            .with_context(|| format!("reading {url}"))
    }
}

impl Source for Steam {
    fn current(&self) -> Result<GameVersion> {
        let info = self.get(&Self::info_url(self.app_id))?;
        let (build, updated) = parse_build(&info, self.app_id, &self.branch)?;
        // A news failure costs the version line, never the badge.
        let version = match self.get(&Self::news_url(self.app_id)) {
            Ok(news) => parse_version(&news, &self.version_re),
            Err(e) => {
                eprintln!("warning: patch version unavailable: {e:#}");
                None
            }
        };
        Ok(GameVersion {
            build,
            version,
            updated,
        })
    }
}

/// `data.<appid>.depots.branches.<branch>.buildid` (and `timeupdated`) out of
/// an api.steamcmd.net response.
pub fn parse_build(json: &str, app_id: u32, branch: &str) -> Result<(String, Option<Timestamp>)> {
    let v: Value = serde_json::from_str(json).context("steamcmd.net response is not JSON")?;
    if let Some(status) = v.get("status").and_then(Value::as_str) {
        if status != "success" {
            bail!("steamcmd.net status {status:?}");
        }
    }
    let br = v
        .get("data")
        .and_then(|d| d.get(app_id.to_string()))
        .and_then(|a| a.get("depots"))
        .and_then(|d| d.get("branches"))
        .and_then(|b| b.get(branch))
        .ok_or_else(|| anyhow!("no depots.branches.{branch} for app {app_id}"))?;
    let build = scalar(br.get("buildid")).ok_or_else(|| anyhow!("branch {branch} has no buildid"))?;
    if build.is_empty() || !build.bytes().all(|b| b.is_ascii_digit()) {
        bail!("buildid {build:?} is not a number");
    }
    let updated = scalar(br.get("timeupdated"))
        .and_then(|s| s.parse::<i64>().ok())
        .map(Timestamp);
    Ok((build, updated))
}

/// The first (newest) announcement whose title matches `re`, capture group 1.
pub fn parse_version(json: &str, re: &Regex) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    v.get("appnews")?
        .get("newsitems")?
        .as_array()?
        .iter()
        .filter_map(|item| item.get("title").and_then(Value::as_str))
        .find_map(|title| re.captures(title)?.get(1).map(|m| m.as_str().to_owned()))
}

/// steamcmd.net gives numbers as strings; accept either.
fn scalar(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}
