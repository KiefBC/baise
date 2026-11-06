//! The mods' own releases, read from GitHub so no version is typed by hand.
//!
//! The workflow does the fetching: for every repo [`repos`] names, it saves
//! `gh release list --json tagName,publishedAt,isDraft,isPrerelease` into one
//! JSON object keyed by `owner/name`. This module only parses that file and
//! matches tags, so it stays pure and testable.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::{Config, ModDecl};
use crate::render::card::ReleaseLine;

/// One entry of `gh release list --json tagName,publishedAt,isDraft,isPrerelease`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhRelease {
    pub tag_name: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub is_prerelease: bool,
}

/// Every repo's releases, keyed by `owner/name`.
pub type Releases = BTreeMap<String, Vec<GhRelease>>;

pub fn parse(text: &str) -> Result<Releases> {
    serde_json::from_str(text)
        .context("GitHub releases file does not parse (expected {\"owner/name\": [releases]})")
}

pub fn load(path: &Path) -> Result<Releases> {
    let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse(&text).with_context(|| format!("in {}", path.display()))
}

/// The repos the workflow must list releases for: each release line's `repo`,
/// or `default` (the repo the workflow runs in) for lines that name none.
/// `default` is always included, because it dates mods that have no tagged lines.
pub fn repos(cfg: &Config, default: &str) -> Vec<String> {
    let mut set = BTreeSet::from([default.to_owned()]);
    for m in &cfg.mods {
        for r in &m.releases {
            if r.tag.is_some() {
                set.insert(r.repo.clone().unwrap_or_else(|| default.to_owned()));
            }
        }
    }
    set.into_iter().collect()
}

/// The newest published, non-draft, non-prerelease release whose tag starts
/// with `prefix`: the tag with the prefix stripped, and its `YYYY-MM-DD`.
pub fn latest<'a>(list: &'a [GhRelease], prefix: &str) -> Option<(&'a str, &'a str)> {
    list.iter()
        .filter(|r| !r.is_draft && !r.is_prerelease)
        .filter_map(|r| {
            let version = r.tag_name.strip_prefix(prefix)?;
            // RFC 3339 UTC: comparing the full timestamp as text orders it, so
            // two releases on the same day still pick the later one.
            let at = r.published_at.as_deref()?;
            let date = at.get(..10)?;
            (!version.is_empty() && crate::clock::is_date(date)).then_some((version, date, at))
        })
        .max_by(|a, b| a.2.cmp(b.2))
        .map(|(version, date, _)| (version, date))
}

/// What one mod's card shows from its releases.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Resolved {
    pub lines: Vec<ReleaseLine>,
    /// The newest of this mod's tagged releases; for a mod with no tagged
    /// lines, the default repo's newest release.
    pub released: Option<String>,
    /// Tagged lines with no matching release, to warn about.
    pub missing: Vec<String>,
}

/// A line's `version`, if set, is drawn as is; otherwise its `tag` prefix is
/// looked up in its `repo` (default: `default`). A tagged line with no match,
/// or with no releases file at all, is left off the card.
pub fn resolve(decl: &ModDecl, gh: Option<&Releases>, default: Option<&str>) -> Resolved {
    let mut out = Resolved::default();
    let mut tagged = false;
    for r in &decl.releases {
        let found = r.tag.as_deref().and_then(|prefix| {
            tagged = true;
            let repo = r.repo.as_deref().or(default)?;
            latest(gh?.get(repo)?, prefix)
        });
        if let Some((_, date)) = found {
            if out.released.as_deref().is_none_or(|d| date > d) {
                out.released = Some(date.to_owned());
            }
        }
        match (r.version.as_deref(), found) {
            (Some(v), _) | (None, Some((v, _))) => out.lines.push(ReleaseLine {
                name: r.name.clone(),
                version: v.to_owned(),
            }),
            (None, None) => out.missing.push(format!(
                "{:?}: no GitHub release tagged {}* in {}",
                r.name,
                r.tag.as_deref().unwrap_or_default(),
                r.repo.as_deref().or(default).unwrap_or("?")
            )),
        }
    }
    if !tagged {
        out.released = default
            .and_then(|repo| gh?.get(repo))
            .and_then(|list| latest(list, ""))
            .map(|(_, date)| date.to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GH: &str = r#"{"KiefBC/desert-mods": [
        {"isDraft":false,"isPrerelease":false,"publishedAt":"2026-11-03T23:18:06Z","tagName":"desert-tooling-v0.16.0"},
        {"isDraft":false,"isPrerelease":false,"publishedAt":"2026-11-02T20:41:54Z","tagName":"desert-tooling-v0.11.0"},
        {"isDraft":false,"isPrerelease":false,"publishedAt":"2026-11-02T20:35:22Z","tagName":"desert-gatherer-dmm-v1.4"},
        {"isDraft":false,"isPrerelease":false,"publishedAt":"2026-11-02T03:15:16Z","tagName":"desert-gatherer-dmm-v1.3"},
        {"isDraft":false,"isPrerelease":true,"publishedAt":"2026-11-05T00:00:00Z","tagName":"desert-tooling-v0.17.0-rc1"},
        {"isDraft":true,"isPrerelease":false,"publishedAt":null,"tagName":"desert-gatherer-dmm-v1.5"},
        {"isDraft":false,"isPrerelease":false,"publishedAt":"2026-09-07T17:53:30Z","tagName":"desert-gatherer-v0.1.1"}
    ], "KiefBC/baise": [
        {"isDraft":false,"isPrerelease":false,"publishedAt":"2026-11-06T10:00:00Z","tagName":"v0.4.0"}
    ]}"#;

    const CFG: &str = r#"
[source]
kind = "manual"
build = "1"

[[mod]]
key = "m"
label = "M"
build = "1"
date = "2026-11-01"
"#;

    fn decl(releases: &str) -> ModDecl {
        let text = format!("{CFG}releases = [{releases}]\n");
        Config::parse(&text).unwrap().mods.remove(0)
    }

    fn lines(r: &Resolved) -> Vec<(&str, &str)> {
        r.lines
            .iter()
            .map(|l| (l.name.as_str(), l.version.as_str()))
            .collect()
    }

    #[test]
    fn newest_published_tag_wins() {
        let gh = parse(GH).unwrap();
        let list = &gh["KiefBC/desert-mods"];
        assert_eq!(latest(list, "desert-tooling-v"), Some(("0.16.0", "2026-11-03")));
        // The draft 1.5 is skipped, 1.4 beats 1.3 from earlier the same day,
        // and the prefix does not match desert-gatherer-v.
        assert_eq!(latest(list, "desert-gatherer-dmm-v"), Some(("1.4", "2026-11-02")));
        assert_eq!(latest(list, "desert-gatherer-v"), Some(("0.1.1", "2026-09-07")));
        assert_eq!(latest(list, "nope-v"), None);
    }

    #[test]
    fn tagged_lines_and_their_date() {
        let gh = parse(GH).unwrap();
        let d = decl(
            r#"{ name = "Desert Tooling", tag = "desert-tooling-v" },
               { name = "Desert Gatherer", tag = "desert-gatherer-dmm-v" }"#,
        );
        let r = resolve(&d, Some(&gh), Some("KiefBC/desert-mods"));
        assert_eq!(
            lines(&r),
            [("Desert Tooling", "0.16.0"), ("Desert Gatherer", "1.4")]
        );
        assert_eq!(r.released.as_deref(), Some("2026-11-03"));
        assert!(r.missing.is_empty());
    }

    #[test]
    fn other_repo_fixed_version_and_missing() {
        let gh = parse(GH).unwrap();
        let d = decl(
            r#"{ name = "baise", tag = "v", repo = "KiefBC/baise" },
               { name = "Pinned", version = "9.9" },
               { name = "Gone", tag = "desert-looter-v" }"#,
        );
        let r = resolve(&d, Some(&gh), Some("KiefBC/desert-mods"));
        assert_eq!(lines(&r), [("baise", "0.4.0"), ("Pinned", "9.9")]);
        assert_eq!(r.released.as_deref(), Some("2026-11-06"));
        assert_eq!(r.missing.len(), 1);
    }

    #[test]
    fn without_a_file_only_fixed_versions_draw() {
        let d = decl(r#"{ name = "A", tag = "a-v" }, { name = "B", version = "1" }"#);
        let r = resolve(&d, None, Some("KiefBC/desert-mods"));
        assert_eq!(lines(&r), [("B", "1")]);
        assert_eq!(r.released, None);
    }

    #[test]
    fn untagged_mod_is_dated_by_the_default_repo() {
        let gh = parse(GH).unwrap();
        let d = decl(r#"{ name = "B", version = "1" }"#);
        let r = resolve(&d, Some(&gh), Some("KiefBC/desert-mods"));
        assert_eq!(r.released.as_deref(), Some("2026-11-03"));
    }

    #[test]
    fn repos_to_fetch() {
        let text = format!(
            "{CFG}releases = [{{ name = \"a\", tag = \"v\", repo = \"KiefBC/baise\" }}, \
             {{ name = \"b\", tag = \"b-v\" }}, {{ name = \"c\", version = \"1\", repo = \"x/y\" }}]\n"
        );
        let cfg = Config::parse(&text).unwrap();
        assert_eq!(
            repos(&cfg, "KiefBC/desert-mods"),
            ["KiefBC/baise", "KiefBC/desert-mods"]
        );
    }
}
