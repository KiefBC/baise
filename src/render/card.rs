//! The `card` layout: 640x184 logical units, drawn at 1.5x (960x276 px)
//! with 3x supersampling, 20 frames of 80 ms.
//!
//! ```text
//! ┃ LIVE STATUS  ·  RELEASED 2026-10-28            ● LIVE   SYNC #00042
//! ┃ ───────────────────────────────────────────────────────────────────
//! ┃ Desert Tooling                                   │      Core: 0.11
//! ┃ W O R K I N G   (letters pulse left to right)    │     Addon: 1.4
//! ┃ ───────────────────────────────────────────────────────────────────
//! ┃ GAME PATCH          VERIFIED            LAST GITHUB SYNC
//! ┃ 2.04.05             2026-11-01          2026-11-02 04:17
//! ┃ BUILD 26361232                          UTC
//! ```

use std::f32::consts::TAU;

use super::canvas::{measure, mix, Canvas, Rgb, Style};
use super::theme::Theme;
use crate::config::Release;
use crate::state::Status;

pub const LOGICAL_W: f32 = 640.0;
pub const LOGICAL_H: f32 = 184.0;
pub const OUT_SCALE: f32 = 1.5;
pub const SUPERSAMPLE: usize = 3;
pub const WIDTH: usize = 960;
pub const HEIGHT: usize = 276;
pub const FRAMES: usize = 20;
/// GIF delay in 10 ms units: 80 ms.
pub const DELAY_CS: u16 = 8;

/// Everything the card shows, already resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardData {
    pub title_prefix: String,
    pub label: String,
    pub status: Status,
    /// The middle column's date.
    pub since: String,
    pub note: String,
    /// The game's current build and patch version.
    pub build: String,
    pub version: Option<String>,
    /// The build the mod declared it was verified on.
    pub verified_build: String,
    pub sync: u64,
    /// `YYYY-MM-DD HH:MM UTC`, or empty before the first sync.
    pub last_sync: String,
    /// The mod's own release lines, at most three are drawn.
    pub releases: Vec<Release>,
    /// `YYYY-MM-DD` of the mod's latest release, drawn after the title prefix.
    pub released: Option<String>,
}

/// Logical units -> supersampled canvas pixels.
const K: f32 = OUT_SCALE * SUPERSAMPLE as f32;

fn st(font: &ab_glyph::FontVec, px: f32, spacing: f32) -> Style<'_> {
    Style { font, px, spacing }
}

fn u(v: f32) -> f32 {
    v * K
}

const MARGIN_L: f32 = 22.0;
const MARGIN_R: f32 = 618.0;
const COLS: [f32; 3] = [MARGIN_L, 228.0, 434.0];

/// The font size that fits `text` into `max_w` logical units, at most `px`.
fn fit(font: &ab_glyph::FontVec, px: f32, spacing: f32, max_w: f32, text: &str) -> f32 {
    let w = measure(st(font, u(px), u(spacing)), text) / K;
    if w <= max_w || w <= 0.0 {
        px
    } else {
        (px * max_w / w).max(px * 0.45)
    }
}

/// The release lines: name font size, version font size, line pitch, and the
/// widest a block may be before it shrinks (all logical units).
const REL_NAME_PX: f32 = 11.0;
const REL_VER_PX: f32 = 14.0;
const REL_PITCH: f32 = 22.0;
const REL_MAX_W: f32 = 190.0;
/// Space between the release block's divider and the text to its left.
const REL_GAP: f32 = 28.0;

/// The release block's lines (at most three), its scale, and its width.
struct Releases<'a> {
    lines: Vec<(String, &'a str)>,
    scale: f32,
    width: f32,
}

impl Releases<'_> {
    /// How much of the title row the block takes, gap included.
    fn reserved(&self) -> f32 {
        if self.lines.is_empty() {
            0.0
        } else {
            self.width + REL_GAP
        }
    }
}

fn releases<'a>(card: &'a CardData, theme: &Theme) -> Releases<'a> {
    let lines: Vec<(String, &str)> = card
        .releases
        .iter()
        .take(crate::config::MAX_RELEASES)
        .map(|r| (format!("{}: ", r.name), r.version.as_str()))
        .collect();
    let widest = lines
        .iter()
        .map(|(name, ver)| {
            (measure(st(&theme.body_font, u(REL_NAME_PX), u(0.6)), name)
                + measure(st(&theme.title_font, u(REL_VER_PX), u(0.4)), ver))
                / K
        })
        .fold(0.0_f32, f32::max);
    let scale = if widest > REL_MAX_W {
        (REL_MAX_W / widest).max(0.5)
    } else {
        1.0
    };
    Releases {
        lines,
        scale,
        width: widest * scale,
    }
}

/// Right-aligned `<name>: <version>` lines, centred between the two rules,
/// with a hairline divider on their left.
fn draw_releases(c: &mut Canvas, rel: &Releases<'_>, theme: &Theme, rule: Rgb) {
    let n = rel.lines.len();
    if n == 0 {
        return;
    }
    let (name_px, ver_px) = (REL_NAME_PX * rel.scale, REL_VER_PX * rel.scale);
    let block_h = (n - 1) as f32 * REL_PITCH + REL_VER_PX;
    let top = 76.0 - block_h * 0.5;
    let div_x = MARGIN_R - rel.width - REL_GAP * 0.5;
    c.fill_rect(u(div_x), u(44.0), u(div_x + 1.0), u(108.0), rule);
    for (i, (name, ver)) in rel.lines.iter().enumerate() {
        let y = top + i as f32 * REL_PITCH;
        let ver_style = st(&theme.title_font, u(ver_px), u(0.4));
        let name_style = st(&theme.body_font, u(name_px), u(0.6));
        let ver_x = u(MARGIN_R) - measure(ver_style, ver);
        let name_x = ver_x - measure(name_style, name);
        // Sit the smaller name on the version's baseline.
        c.text(
            name_style,
            name_x,
            u(y + (ver_px - name_px) * 0.8),
            name,
            theme.dim,
        );
        c.text(ver_style, ver_x, u(y), ver, theme.text);
    }
}

/// The parts of the card that do not move.
fn base(card: &CardData, theme: &Theme) -> Canvas {
    let (w, h) = (WIDTH * SUPERSAMPLE, HEIGHT * SUPERSAMPLE);
    let mut c = Canvas::new(w, h, theme.background);
    let status = theme.status_color(card.status);
    let rule = mix(theme.background, theme.dim, 0.35);

    // Frame: a hairline border and the status bar down the left edge.
    let edge = u(1.0);
    c.fill_rect(0.0, 0.0, w as f32, edge, rule);
    c.fill_rect(0.0, h as f32 - edge, w as f32, h as f32, rule);
    c.fill_rect(w as f32 - edge, 0.0, w as f32, h as f32, rule);
    c.fill_rect(0.0, 0.0, u(5.0), h as f32, status);

    // Header.
    let body = &theme.body_font;
    let title = &theme.title_font;
    let prefix_style = st(title, u(10.0), u(1.6));
    c.text(
        prefix_style,
        u(MARGIN_L),
        u(13.0),
        &card.title_prefix,
        theme.accent,
    );
    if let Some(date) = &card.released {
        let x = u(MARGIN_L) + measure(prefix_style, &card.title_prefix) + u(10.0);
        let released = format!("·  RELEASED {date}");
        c.text(st(body, u(10.0), u(1.2)), x, u(13.0), &released, theme.dim);
    }
    let sync = format!("SYNC #{:05}", card.sync);
    let sync_w = measure(st(body, u(10.0), u(1.2)), &sync);
    c.text(
        st(body, u(10.0), u(1.2)),
        u(MARGIN_R) - sync_w,
        u(13.0),
        &sync,
        theme.dim,
    );
    let live_w = measure(st(title, u(10.0), u(1.2)), "LIVE");
    let live_x = u(MARGIN_R) - sync_w - u(18.0) - live_w;
    c.text(st(title, u(10.0), u(1.2)), live_x, u(13.0), "LIVE", theme.text);

    c.fill_rect(u(MARGIN_L), u(33.0), u(MARGIN_R), u(34.0), rule);

    // Release lines on the right, then the mod title in what is left.
    let rel = releases(card, theme);
    draw_releases(&mut c, &rel, theme, rule);
    let label_px = fit(
        title,
        28.0,
        0.0,
        MARGIN_R - MARGIN_L - rel.reserved(),
        &card.label,
    );
    c.text(
        st(title, u(label_px), 0.0),
        u(MARGIN_L),
        u(40.0 + (28.0 - label_px) * 0.5),
        &card.label,
        theme.text,
    );

    c.fill_rect(u(MARGIN_L), u(117.0), u(MARGIN_R), u(118.0), rule);

    // Columns.
    let (patch, patch_sub) = match &card.version {
        Some(v) => (v.clone(), format!("BUILD {}", card.build)),
        None => (card.build.clone(), "STEAM BUILD".to_owned()),
    };
    let since_sub = match card.status {
        Status::Untested | Status::Broken if card.verified_build != card.build => {
            format!("VERIFIED ON {}", card.verified_build)
        }
        _ => String::new(),
    };
    let (sync_value, sync_sub) = match card.last_sync.rsplit_once(' ') {
        Some((dt, zone)) => (dt.to_owned(), zone.to_owned()),
        None if card.last_sync.is_empty() => ("-".to_owned(), String::new()),
        None => (card.last_sync.clone(), String::new()),
    };
    let columns = [
        ("GAME PATCH", patch, patch_sub),
        (card.status.since_label(), card.since.clone(), since_sub),
        ("LAST GITHUB SYNC", sync_value, sync_sub),
    ];
    for (x, (label, value, sub)) in COLS.iter().zip(columns) {
        c.text(st(body, u(9.0), u(1.4)), u(*x), u(127.0), label, theme.dim);
        let px = fit(title, 17.0, 0.4, 196.0, &value);
        c.text(st(title, u(px), u(0.4)), u(*x), u(140.0), &value, theme.text);
        c.text(
            st(body, u(8.5), u(1.2)),
            u(*x),
            u(163.0),
            &sub,
            mix(theme.background, theme.dim, 0.8),
        );
    }
    c
}

/// Brightness of letter `k` of `n` at time `t` in [0, 1): a wave travelling
/// left to right, one period per loop.
pub fn pulse(t: f32, k: usize, n: usize) -> f32 {
    0.5 + 0.5 * (TAU * (t - k as f32 / (n as f32 + 3.0))).cos()
}

/// The LIVE dot: on for most of the loop, a quick fade out and back.
pub fn blink(t: f32) -> f32 {
    let x = 0.5 + 0.5 * (TAU * t).cos();
    (x * 1.6).min(1.0)
}

/// Draws the 20 frames at 960x276.
pub fn render(card: &CardData, theme: &Theme) -> Vec<Canvas> {
    let base = base(card, theme);
    let status = theme.status_color(card.status);
    let low = mix(theme.background, status, 0.42);
    let title = &theme.title_font;
    let body = &theme.body_font;

    let headline = card.status.headline();
    let note = if card.note.is_empty() {
        String::new()
    } else {
        format!("//  {}", card.note)
    };
    let room = MARGIN_R - MARGIN_L - releases(card, theme).reserved();
    let head_px = fit(title, 21.0, 2.2, room, headline);
    let head_w = measure(st(title, u(head_px), u(2.2)), headline) / K;
    let note_room = room - head_w - 14.0;
    let note_px = fit(body, 11.0, 0.6, note_room.max(1.0), &note);
    let n = headline.chars().count();

    let sync_w = measure(st(body, u(10.0), u(1.2)), &format!("SYNC #{:05}", card.sync));
    let live_w = measure(st(title, u(10.0), u(1.2)), "LIVE");
    let dot_x = u(MARGIN_R) - sync_w - u(18.0) - live_w - u(8.0);

    (0..FRAMES)
        .map(|i| {
            let t = i as f32 / FRAMES as f32;
            let mut c = base.clone();
            c.text_with(
                st(title, u(head_px), u(2.2)),
                u(MARGIN_L),
                u(78.0),
                headline,
                |k| mix(low, status, pulse(t, k, n)),
            );
            if !note.is_empty() && note_room > 40.0 {
                c.text(
                    st(body, u(note_px), u(0.6)),
                    u(MARGIN_L + head_w + 14.0),
                    u(86.0),
                    &note,
                    theme.dim,
                );
            }
            let dot = mix(mix(theme.background, status, 0.18), status, blink(t));
            c.circle(dot_x, u(19.5), u(3.6), dot);
            c.downsample(SUPERSAMPLE)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_travels_left_to_right() {
        // At t = 0 the first letter is brightest; a quarter later the peak has moved right.
        assert!((pulse(0.0, 0, 7) - 1.0).abs() < 1e-6);
        assert!(pulse(0.0, 0, 7) > pulse(0.0, 3, 7));
        assert!(pulse(0.3, 3, 7) > pulse(0.3, 0, 7));
        for k in 0..7 {
            for i in 0..FRAMES {
                let p = pulse(i as f32 / FRAMES as f32, k, 7);
                assert!((0.0..=1.0).contains(&p));
            }
        }
    }

    #[test]
    fn blink_stays_in_range_and_dips() {
        assert!((blink(0.0) - 1.0).abs() < 1e-6);
        assert!(blink(0.5) < 0.01);
        assert!((0..FRAMES).all(|i| (0.0..=1.0).contains(&blink(i as f32 / FRAMES as f32))));
    }

    #[test]
    fn dimensions_match_constants() {
        assert_eq!((LOGICAL_W * OUT_SCALE) as usize, WIDTH);
        assert_eq!((LOGICAL_H * OUT_SCALE) as usize, HEIGHT);
    }
}
