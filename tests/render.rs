//! Decode the GIF back: 960x276, 20 frames, 80 ms, loops forever, same bytes every time.

// Helpers outside #[test] fns are test code too.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use baise::outputs;
use baise::render::card::{self, CardData};
use baise::render::Theme;
use baise::state::Status;

fn sample(status: Status) -> CardData {
    CardData {
        title_prefix: "LIVE STATUS".into(),
        label: "Desert Tooling".into(),
        status,
        since: "2026-11-01".into(),
        note: "a note".into(),
        build: "26361232".into(),
        version: Some("2.04.05".into()),
        verified_build: "26361232".into(),
        sync: 42,
        last_sync: "2026-11-02 04:15 UTC".into(),
        releases: Vec::new(),
    }
}

fn gif_bytes(theme: &Theme, status: Status) -> Vec<u8> {
    let frames = card::render(&sample(status), theme);
    outputs::gif::encode(&frames, theme.background, card::DELAY_CS).unwrap()
}

#[test]
fn gif_decodes_to_the_spec() {
    let theme = Theme::builtin("crimson-desert").unwrap();
    let bytes = gif_bytes(&theme, Status::Untested);

    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(bytes.as_slice()).unwrap();
    assert_eq!((dec.width(), dec.height()), (960, 276));

    let mut frames = 0;
    let mut first_pixel = None;
    while let Some(frame) = dec.read_next_frame().unwrap() {
        assert_eq!(frame.delay, 8, "80 ms per frame");
        if frames == 0 {
            assert_eq!((frame.width, frame.height), (960, 276), "frame 0 covers the card");
            // Top-right corner is the hairline border; one pixel in is background.
            let i = (276 / 2 * 960 + 900) * 4;
            first_pixel = Some([frame.buffer[i], frame.buffer[i + 1], frame.buffer[i + 2]]);
        }
        frames += 1;
    }
    assert_eq!(frames, 20);
    assert_eq!(dec.repeat(), gif::Repeat::Infinite);
    assert_eq!(
        first_pixel,
        Some(theme.background),
        "palette slot 255 is the exact background"
    );
}

#[test]
fn gif_is_deterministic_and_status_dependent() {
    let theme = Theme::builtin("nexus").unwrap();
    let a = gif_bytes(&theme, Status::Working);
    let b = gif_bytes(&theme, Status::Working);
    assert_eq!(a, b);
    assert_ne!(a, gif_bytes(&theme, Status::Broken));
}

#[test]
fn frames_animate() {
    let theme = Theme::builtin("nexus").unwrap();
    let frames = card::render(&sample(Status::Working), &theme);
    assert_eq!(frames.len(), card::FRAMES);
    assert!(frames
        .iter()
        .all(|f| (f.width, f.height) == (card::WIDTH, card::HEIGHT)));
    assert_ne!(frames[0], frames[5], "the status line pulses");
}

#[test]
fn png_and_shields_outputs() {
    let theme = Theme::builtin("nexus").unwrap();
    let dir = std::env::temp_dir().join(format!("baise-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    outputs::write_mod(&dir, "desert-tooling", &sample(Status::Working), &theme).unwrap();

    let png = std::fs::read(dir.join("desert-tooling.png")).unwrap();
    let dec = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
    assert_eq!((dec.info().width, dec.info().height), (960, 276));

    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("desert-tooling.json")).unwrap()).unwrap();
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["label"], "Desert Tooling");
    assert_eq!(json["message"], "working · 2.04.05");
    assert_eq!(json["color"], "57c754");
    std::fs::remove_dir_all(&dir).unwrap();
}
