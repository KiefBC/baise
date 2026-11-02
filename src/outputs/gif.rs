//! Frames -> one animated GIF: a single shared palette (NeuQuant over two
//! frames, 255 colours) with slot 255 pinned to the background so the large
//! flat areas never shimmer, no dithering, infinite loop. Deterministic: the
//! same frames always give the same bytes.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use color_quant::NeuQuant;
use gif::{DisposalMethod, Encoder, Frame, Repeat};

use crate::render::canvas::{Canvas, Rgb};

/// The palette slot reserved for the background colour.
pub const BG_INDEX: u8 = 255;

pub fn encode(frames: &[Canvas], background: Rgb, delay_cs: u16) -> Result<Vec<u8>> {
    let Some(first) = frames.first() else {
        bail!("no frames to encode")
    };
    let (w, h) = (first.width, first.height);
    if frames.iter().any(|f| f.width != w || f.height != h) {
        bail!("frames differ in size");
    }
    let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) else {
        bail!("{w}x{h} is too large for a GIF");
    };

    // Train on frame 0 and the middle frame: between them they hold every
    // colour the pulse reaches.
    let mut sample = first.to_rgba();
    if let Some(mid) = frames.get(frames.len() / 2) {
        sample.extend(mid.to_rgba());
    }
    let nq = NeuQuant::new(10, 255, &sample);
    let mut palette = nq.color_map_rgb();
    palette.resize(255 * 3, 0);
    palette.extend_from_slice(&background);

    let mut encoder = Encoder::new(Vec::new(), w16, h16, &palette).context("gif header")?;
    encoder
        .set_repeat(Repeat::Infinite)
        .context("gif loop extension")?;

    let mut cache: HashMap<Rgb, u8> = HashMap::new();
    let mut prev: Option<Vec<u8>> = None;
    for canvas in frames {
        let indices: Vec<u8> = canvas
            .pixels
            .iter()
            .map(|&p| {
                if p == background {
                    return BG_INDEX;
                }
                *cache.entry(p).or_insert_with(|| {
                    let i = nq.index_of(&[p[0], p[1], p[2], 255]);
                    u8::try_from(i).unwrap_or(BG_INDEX).min(254)
                })
            })
            .collect();
        // Only the status line and the LIVE dot move: after the first frame,
        // each frame is just the rectangle that changed, drawn over the last.
        let rect = match &prev {
            None => Rect { x: 0, y: 0, w, h },
            Some(p) => changed(p, &indices, w, h),
        };
        let mut sub = Vec::with_capacity(rect.w * rect.h);
        for y in rect.y..rect.y + rect.h {
            let row = y * w;
            sub.extend_from_slice(indices.get(row + rect.x..row + rect.x + rect.w).unwrap_or(&[]));
        }
        let (Ok(fx), Ok(fy), Ok(fw), Ok(fh)) = (
            u16::try_from(rect.x),
            u16::try_from(rect.y),
            u16::try_from(rect.w),
            u16::try_from(rect.h),
        ) else {
            bail!("frame rectangle out of range");
        };
        if sub.len() != rect.w * rect.h {
            bail!("frame rectangle out of range");
        }
        let mut frame = Frame::from_indexed_pixels(fw, fh, sub, None);
        frame.left = fx;
        frame.top = fy;
        frame.delay = delay_cs;
        frame.dispose = DisposalMethod::Keep;
        encoder.write_frame(&frame).context("gif frame")?;
        prev = Some(indices);
    }
    encoder.into_inner().context("gif trailer")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

/// The bounding box of the pixels that differ; 1x1 at the origin when none do.
fn changed(a: &[u8], b: &[u8], w: usize, h: usize) -> Rect {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if a.get(i) != b.get(i) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 == usize::MAX {
        return Rect {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        };
    }
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0 + 1,
        h: y1 - y0 + 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_rect() {
        let a = [0u8; 12];
        let mut b = a;
        assert_eq!(
            changed(&a, &b, 4, 3),
            Rect {
                x: 0,
                y: 0,
                w: 1,
                h: 1
            }
        );
        b[5] = 1; // (1,1)
        b[11] = 1; // (3,2)
        assert_eq!(
            changed(&a, &b, 4, 3),
            Rect {
                x: 1,
                y: 1,
                w: 3,
                h: 2
            }
        );
    }
}
