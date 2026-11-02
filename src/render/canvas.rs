//! An opaque RGB canvas drawn at a supersampled size and box-filtered down.
//! Text goes through ab_glyph's coverage rasteriser, blended onto the canvas.

use ab_glyph::{Font, FontVec, Glyph, PxScale, ScaleFont};

pub type Rgb = [u8; 3];

/// Linear blend: `t = 0` is `a`, `t = 1` is `b`.
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| -> u8 {
        let v = f32::from(x) + (f32::from(y) - f32::from(x)) * t;
        // Clamped to 0..=255 first, so the cast cannot truncate.
        v.round().clamp(0.0, 255.0) as u8
    };
    [ch(a[0], b[0]), ch(a[1], b[1]), ch(a[2], b[2])]
}

/// A font at a size, with extra letter spacing, all in canvas pixels.
#[derive(Clone, Copy)]
pub struct Style<'a> {
    pub font: &'a FontVec,
    pub px: f32,
    pub spacing: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Rgb>,
}

impl Canvas {
    pub fn new(width: usize, height: usize, fill: Rgb) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; width * height],
        }
    }

    fn blend(&mut self, x: i64, y: i64, color: Rgb, alpha: f32) {
        if x < 0 || y < 0 {
            return;
        }
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        if let Some(p) = self.pixels.get_mut(y * self.width + x) {
            *p = mix(*p, color, alpha);
        }
    }

    /// Fills the pixel rectangle `[x0, x1) x [y0, y1)`, clipped to the canvas.
    pub fn fill_rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgb) {
        let clamp_x = |v: f32| (v.round().max(0.0) as usize).min(self.width);
        let clamp_y = |v: f32| (v.round().max(0.0) as usize).min(self.height);
        let (xa, xb, ya, yb) = (clamp_x(x0), clamp_x(x1), clamp_y(y0), clamp_y(y1));
        for y in ya..yb {
            let row = y * self.width;
            if let Some(span) = self.pixels.get_mut(row + xa..row + xb) {
                span.fill(color);
            }
        }
    }

    /// A filled circle with a one-pixel soft edge.
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: Rgb) {
        let x0 = (cx - r - 1.0).floor() as i64;
        let x1 = (cx + r + 1.0).ceil() as i64;
        let y0 = (cy - r - 1.0).floor() as i64;
        let y1 = (cy + r + 1.0).ceil() as i64;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt();
                let a = (r + 0.5 - d).clamp(0.0, 1.0);
                if a > 0.0 {
                    self.blend(x, y, color, a);
                }
            }
        }
    }

    /// Draws `text` with its top (the font's ascent line) at `y`, starting at
    /// `x`, with `spacing` extra pixels between letters. `color(i)` gives the
    /// colour of the `i`-th character. Returns the advance width.
    pub fn text_with(
        &mut self,
        style: Style<'_>,
        x: f32,
        y: f32,
        text: &str,
        mut color: impl FnMut(usize) -> Rgb,
    ) -> f32 {
        let Style { font, px, spacing } = style;
        let scaled = font.as_scaled(PxScale::from(px));
        let baseline = y + scaled.ascent();
        let mut caret = x;
        let mut prev = None;
        for (i, ch) in text.chars().enumerate() {
            let id = scaled.glyph_id(ch);
            if let Some(p) = prev {
                caret += scaled.kern(p, id);
            }
            let glyph: Glyph = id.with_scale_and_position(px, ab_glyph::point(caret, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let b = outlined.px_bounds();
                let c = color(i);
                let (bx, by) = (b.min.x as i64, b.min.y as i64);
                outlined.draw(|gx, gy, cov| {
                    self.blend(bx + i64::from(gx), by + i64::from(gy), c, cov);
                });
            }
            caret += scaled.h_advance(id) + spacing;
            prev = Some(id);
        }
        caret - x - if text.is_empty() { 0.0 } else { spacing }
    }

    pub fn text(&mut self, style: Style<'_>, x: f32, y: f32, text: &str, color: Rgb) -> f32 {
        self.text_with(style, x, y, text, |_| color)
    }

    /// Box-filters the canvas down by `factor` in each direction.
    pub fn downsample(&self, factor: usize) -> Canvas {
        let factor = factor.max(1);
        let (w, h) = (self.width / factor, self.height / factor);
        let n = (factor * factor) as u32;
        let mut out = Vec::with_capacity(w * h);
        for oy in 0..h {
            for ox in 0..w {
                let mut acc = [0u32; 3];
                for sy in 0..factor {
                    let row = (oy * factor + sy) * self.width + ox * factor;
                    for p in self.pixels.get(row..row + factor).unwrap_or(&[]) {
                        for (a, v) in acc.iter_mut().zip(p) {
                            *a += u32::from(*v);
                        }
                    }
                }
                let avg = |a: u32| u8::try_from((a + n / 2) / n).unwrap_or(u8::MAX);
                out.push([avg(acc[0]), avg(acc[1]), avg(acc[2])]);
            }
        }
        Canvas {
            width: w,
            height: h,
            pixels: out,
        }
    }

    /// Row-major RGBA bytes, alpha 255.
    pub fn to_rgba(&self) -> Vec<u8> {
        self.pixels.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()
    }

    /// Row-major RGB bytes.
    pub fn to_rgb(&self) -> Vec<u8> {
        self.pixels.iter().flatten().copied().collect()
    }
}

/// The advance width `text_with` would return, without drawing.
pub fn measure(style: Style<'_>, text: &str) -> f32 {
    let Style { font, px, spacing } = style;
    let scaled = font.as_scaled(PxScale::from(px));
    let mut w = 0.0;
    let mut prev = None;
    let mut n = 0usize;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            w += scaled.kern(p, id);
        }
        w += scaled.h_advance(id);
        prev = Some(id);
        n += 1;
    }
    w + spacing * n.saturating_sub(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_endpoints() {
        assert_eq!(mix([0, 0, 0], [255, 100, 10], 0.0), [0, 0, 0]);
        assert_eq!(mix([0, 0, 0], [255, 100, 10], 1.0), [255, 100, 10]);
        assert_eq!(mix([0, 0, 0], [255, 100, 10], 2.0), [255, 100, 10]);
        assert_eq!(mix([0, 0, 0], [200, 100, 10], 0.5), [100, 50, 5]);
    }

    #[test]
    fn rect_is_clipped_and_downsample_averages() {
        let mut c = Canvas::new(6, 6, [0, 0, 0]);
        c.fill_rect(-5.0, -5.0, 3.0, 3.0, [90, 90, 90]);
        c.fill_rect(5.0, 5.0, 100.0, 100.0, [9, 9, 9]);
        let d = c.downsample(3);
        assert_eq!((d.width, d.height), (2, 2));
        assert_eq!(d.pixels, vec![[90; 3], [0; 3], [0; 3], [1; 3]]);
    }
}
