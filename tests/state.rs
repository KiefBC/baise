//! The rule table: equal, moved, override, first-seen date.

// Helpers outside #[test] fns are test code too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use mod_status::clock::Timestamp;
use mod_status::config::ModDecl;
use mod_status::source::GameVersion;
use mod_status::state::{resolve, Origin, State, Status};

// 2026-11-01 14:20 UTC and the next day.
const DAY1: Timestamp = Timestamp(1_793_542_812);
const DAY2: Timestamp = Timestamp(1_793_542_812 + 86_400);

fn decl(on_update: Status) -> ModDecl {
    ModDecl {
        key: "desert-tooling".into(),
        label: "Desert Tooling".into(),
        build: "26361232".into(),
        date: "2026-10-30".into(),
        status: Status::Working,
        on_update,
        note: String::new(),
    }
}

fn game(build: &str) -> GameVersion {
    GameVersion {
        build: build.into(),
        version: Some("2.04.05".into()),
        updated: None,
    }
}

#[test]
fn equal_build_shows_declared_status_and_date() {
    let mut st = State::default();
    st.observe(&game("26361232"), DAY1, None);
    let r = resolve(&decl(Status::Untested), "26361232", &st);
    assert_eq!(
        (r.status, r.since.as_str(), r.origin),
        (Status::Working, "2026-10-30", Origin::Declared)
    );
}

#[test]
fn moved_build_flips_to_on_update_dated_first_seen() {
    for on_update in [Status::Untested, Status::Broken] {
        let mut st = State::default();
        st.observe(&game("99999999"), DAY1, None);
        st.observe(&game("99999999"), DAY2, None);
        let r = resolve(&decl(on_update), "99999999", &st);
        assert_eq!(r.status, on_update);
        assert_eq!(r.since, "2026-11-01", "first seen, not the latest sync");
        assert_eq!(r.origin, Origin::GameMoved);
    }
}

#[test]
fn declared_wip_or_broken_is_shown_while_build_matches() {
    for status in [Status::Wip, Status::Broken] {
        let mut d = decl(Status::Untested);
        d.status = status;
        let r = resolve(&d, "26361232", &State::default());
        assert_eq!(r.status, status);
    }
}

#[test]
fn override_applies_only_on_its_build() {
    let mut st = State::default();
    st.observe(&game("99999999"), DAY1, None);
    st.set_override("desert-tooling", Some(Status::Broken), None, DAY2)
        .unwrap();
    let r = resolve(&decl(Status::Untested), "99999999", &st);
    assert_eq!(
        (r.status, r.since.as_str(), r.origin),
        (Status::Broken, "2026-11-02", Origin::Override)
    );

    // The next update makes the override stale; the rule takes over again.
    st.observe(&game("100000000"), DAY2, None);
    let r = resolve(&decl(Status::Untested), "100000000", &st);
    assert_eq!((r.status, r.origin), (Status::Untested, Origin::GameMoved));

    // Clearing it.
    st.set_override("desert-tooling", Some(Status::Working), Some("100000000"), DAY2)
        .unwrap();
    assert_eq!(
        resolve(&decl(Status::Untested), "100000000", &st).status,
        Status::Working
    );
    st.set_override("desert-tooling", None, None, DAY2).unwrap();
    assert_eq!(
        resolve(&decl(Status::Untested), "100000000", &st).status,
        Status::Untested
    );
}

#[test]
fn override_needs_a_build() {
    let mut st = State::default();
    assert!(st
        .set_override("desert-tooling", Some(Status::Broken), None, DAY1)
        .is_err());
}

#[test]
fn sync_counter_and_versions() {
    let mut st = State::default();
    st.observe(&game("1"), DAY1, None);
    st.observe(&game("1"), DAY1, None);
    assert_eq!(st.sync, 2);
    st.observe(&game("1"), DAY1, Some(500));
    assert_eq!(st.sync, 500);

    // News outage on the same build keeps the version; a new build drops it.
    st.observe(
        &GameVersion {
            build: "1".into(),
            version: None,
            updated: None,
        },
        DAY2,
        None,
    );
    assert_eq!(st.latest_version.as_deref(), Some("2.04.05"));
    st.observe(
        &GameVersion {
            build: "2".into(),
            version: None,
            updated: None,
        },
        DAY2,
        None,
    );
    assert_eq!(st.latest_version, None);
    assert_eq!(st.last_sync(), Some(DAY2));
}

#[test]
fn state_json_round_trips_and_tolerates_missing_fields() {
    let mut st = State::default();
    st.observe(&game("26361232"), DAY1, Some(7));
    st.set_override("desert-tooling", Some(Status::Wip), None, DAY1)
        .unwrap();
    let text = serde_json::to_string(&st).unwrap();
    assert_eq!(serde_json::from_str::<State>(&text).unwrap(), st);
    assert_eq!(serde_json::from_str::<State>("{}").unwrap(), State::default());
}

#[test]
fn status_strings_round_trip() {
    for s in Status::ALL {
        assert_eq!(s.as_str().parse::<Status>().unwrap(), s);
    }
    assert!("fine".parse::<Status>().is_err());
}
