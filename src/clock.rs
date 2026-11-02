//! UTC dates without a date crate: the kit only ever prints `YYYY-MM-DD` and
//! `YYYY-MM-DD HH:MM UTC`, and takes "now" as a parameter everywhere so the
//! rule and the renderer stay deterministic under test.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch, UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// The current wall-clock time. A clock before 1970 reads as the epoch.
    pub fn now() -> Self {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        Self(secs)
    }

    fn parts(self) -> (i64, u32, u32, u32, u32) {
        let days = self.0.div_euclid(86_400);
        let rem = self.0.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        let hh = u32::try_from(rem / 3600).unwrap_or(0);
        let mm = u32::try_from((rem % 3600) / 60).unwrap_or(0);
        (y, m, d, hh, mm)
    }

    /// `2026-11-02`
    pub fn date(self) -> String {
        let (y, m, d, _, _) = self.parts();
        format!("{y:04}-{m:02}-{d:02}")
    }

    /// `2026-11-02 04:17 UTC`
    pub fn datetime(self) -> String {
        let (y, m, d, hh, mm) = self.parts();
        format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02} UTC")
    }

    /// `2026-11-02T04:17:00Z`, what state.json stores.
    pub fn rfc3339(self) -> String {
        let (y, m, d, hh, mm) = self.parts();
        let ss = self.0.rem_euclid(60);
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
    }

    /// Parses what [`Timestamp::rfc3339`] writes (seconds optional).
    pub fn parse_rfc3339(s: &str) -> Option<Self> {
        let s = s.strip_suffix('Z')?;
        let (date, time) = s.split_once('T')?;
        let mut d = date.splitn(3, '-');
        let y: i64 = d.next()?.parse().ok()?;
        let m: u32 = d.next()?.parse().ok()?;
        let day: u32 = d.next()?.parse().ok()?;
        let mut t = time.splitn(3, ':');
        let hh: i64 = t.next()?.parse().ok()?;
        let mm: i64 = t.next()?.parse().ok()?;
        let ss: i64 = t.next().map_or(Some(0), |v| v.parse().ok())?;
        if !(1..=12).contains(&m) || !(1..=31).contains(&day) || hh > 23 || mm > 59 || ss > 60 {
            return None;
        }
        Some(Self(
            days_from_civil(y, m, day) * 86_400 + hh * 3600 + mm * 60 + ss,
        ))
    }
}

/// Is `s` a `YYYY-MM-DD` date?
pub fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            _ => c.is_ascii_digit(),
        })
}

// Howard Hinnant's civil-from-days / days-from-civil, proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, u32::try_from(m).unwrap_or(1), u32::try_from(d).unwrap_or(1))
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants() {
        assert_eq!(Timestamp(0).datetime(), "1970-01-01 00:00 UTC");
        // The steamcmd.net fixture's timeupdated for build 26361232.
        assert_eq!(Timestamp(1_793_542_812).datetime(), "2026-11-01 14:20 UTC");
        assert_eq!(Timestamp(951_782_400).date(), "2000-02-29");
    }

    #[test]
    fn rfc3339_round_trip() {
        for t in [0, 1_793_542_812, 951_782_400, 4_102_444_799] {
            let ts = Timestamp(t);
            assert_eq!(Timestamp::parse_rfc3339(&ts.rfc3339()), Some(ts));
        }
        assert_eq!(
            Timestamp::parse_rfc3339("2026-11-01T14:20Z"),
            Some(Timestamp(1_793_542_800))
        );
        assert_eq!(Timestamp::parse_rfc3339("2026-13-01T00:00Z"), None);
        assert_eq!(Timestamp::parse_rfc3339("garbage"), None);
    }

    #[test]
    fn dates() {
        assert!(is_date("2026-11-01"));
        assert!(!is_date("2026-1-01"));
        assert!(!is_date("2026/11/01"));
    }
}
