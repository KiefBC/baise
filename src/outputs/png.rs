//! Frame 0 as a PNG, for places that refuse animated images.

use anyhow::{Context, Result};

use crate::render::canvas::Canvas;

pub fn encode(frame: &Canvas) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let w = u32::try_from(frame.width).context("png width")?;
        let h = u32::try_from(frame.height).context("png height")?;
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::High);
        let mut writer = enc.write_header().context("png header")?;
        writer.write_image_data(&frame.to_rgb()).context("png data")?;
        writer.finish().context("png trailer")?;
    }
    Ok(out)
}
