//! The Steam parsers against responses recorded from the live endpoints on
//! 2026-11-02 (Crimson Desert, app 3321460).

// Helpers outside #[test] fns are test code too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use mod_status::clock::Timestamp;
use mod_status::config::DEFAULT_VERSION_REGEX;
use mod_status::source::steam::{parse_build, parse_version};
use regex::Regex;

const INFO: &str = include_str!("fixtures/steamcmd_3321460.json");
const NEWS: &str = include_str!("fixtures/news_3321460.json");

#[test]
fn build_from_steamcmd_net() {
    let (build, updated) = parse_build(INFO, 3321460, "public").unwrap();
    assert_eq!(build, "26361232");
    assert_eq!(updated, Some(Timestamp(1_793_542_812)));
}

#[test]
fn wrong_app_or_branch_is_an_error() {
    assert!(parse_build(INFO, 1, "public").is_err());
    assert!(parse_build(INFO, 3321460, "no-such-branch").is_err());
}

#[test]
fn failed_or_garbled_responses_are_errors() {
    assert!(parse_build(r#"{"status":"failed","data":{}}"#, 1, "public").is_err());
    assert!(parse_build("<html>502</html>", 1, "public").is_err());
    let not_number =
        r#"{"status":"success","data":{"1":{"depots":{"branches":{"public":{"buildid":"abc"}}}}}}"#;
    assert!(parse_build(not_number, 1, "public").is_err());
    let numeric = r#"{"data":{"1":{"depots":{"branches":{"public":{"buildid":42,"timeupdated":7}}}}}}"#;
    assert_eq!(
        parse_build(numeric, 1, "public").unwrap(),
        ("42".into(), Some(Timestamp(7)))
    );
}

#[test]
fn version_from_newest_patch_notes() {
    let re = Regex::new(DEFAULT_VERSION_REGEX).unwrap();
    assert_eq!(parse_version(NEWS, &re).as_deref(), Some("2.04.05"));
}

#[test]
fn version_skips_non_matching_titles() {
    let re = Regex::new(DEFAULT_VERSION_REGEX).unwrap();
    let news = r#"{"appnews":{"newsitems":[
        {"title":"Vote for us!"},
        {"title":"Patch Notes Version 2.04.00 (All Platforms Hotfix)"}]}}"#;
    assert_eq!(parse_version(news, &re).as_deref(), Some("2.04.00"));
    assert_eq!(parse_version(r#"{"appnews":{"newsitems":[]}}"#, &re), None);
    assert_eq!(parse_version("not json", &re), None);
}
