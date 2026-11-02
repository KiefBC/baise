//! Themes are data: a TOML file of colours and a font. Two are built in;
//! anything else is a path to a `.toml` of the same shape.

use std::path::{Path, PathBuf};

use ab_glyph::{FontVec, VariableFont};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use super::canvas::Rgb;
use crate::state::Status;

/// Exo 2 by Natanael Gama, SIL Open Font License 1.1 (fonts/OFL.txt).
pub const EXO2: &[u8] = include_bytes!("../../fonts/Exo2-Variable.ttf");

pub const BUILTIN: [(&str, &str); 2] = [
    ("nexus", include_str!("../../themes/nexus.toml")),
    ("crimson-desert", include_str!("../../themes/crimson-desert.toml")),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    name: String,
    colors: ColorsFile,
    #[serde(default)]
    font: FontFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ColorsFile {
    background: String,
    accent: String,
    text: String,
    dim: String,
    working: String,
    untested: String,
    wip: String,
    broken: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct FontFile {
    #[serde(default = "default_font")]
    file: String,
    #[serde(default = "default_title_weight")]
    title_weight: f32,
    #[serde(default = "default_body_weight")]
    body_weight: f32,
}

impl Default for FontFile {
    fn default() -> Self {
        Self {
            file: default_font(),
            title_weight: default_title_weight(),
            body_weight: default_body_weight(),
        }
    }
}

fn default_font() -> String {
    "exo2".into()
}
fn default_title_weight() -> f32 {
    700.0
}
fn default_body_weight() -> f32 {
    500.0
}

pub struct Theme {
    pub name: String,
    pub background: Rgb,
    pub accent: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub working: Rgb,
    pub untested: Rgb,
    pub wip: Rgb,
    pub broken: Rgb,
    /// Mod title and status line.
    pub title_font: FontVec,
    /// Labels and values.
    pub body_font: FontVec,
}

impl std::fmt::Debug for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Theme")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Theme {
    pub fn status_color(&self, s: Status) -> Rgb {
        match s {
            Status::Working => self.working,
            Status::Untested => self.untested,
            Status::Wip => self.wip,
            Status::Broken => self.broken,
        }
    }

    /// A built-in theme by name.
    pub fn builtin(name: &str) -> Result<Self> {
        let (_, text) = BUILTIN
            .iter()
            .find(|(n, _)| *n == name)
            .ok_or_else(|| anyhow!("no built-in theme {name:?}"))?;
        Self::parse(text, None)
    }

    /// `render.theme` from a config: a built-in name, or a path resolved
    /// against `base` (the config file's directory).
    pub fn resolve(spec: &str, base: &Path) -> Result<Self> {
        if BUILTIN.iter().any(|(n, _)| *n == spec) {
            return Self::builtin(spec);
        }
        let path = base.join(spec);
        if !spec.ends_with(".toml") {
            let names: Vec<&str> = BUILTIN.iter().map(|(n, _)| *n).collect();
            bail!(
                "theme {spec:?} is neither built in ({}) nor a .toml path",
                names.join(", ")
            );
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read theme {}", path.display()))?;
        Self::parse(&text, path.parent().map(Path::to_path_buf))
            .with_context(|| format!("in theme {}", path.display()))
    }

    /// Parses a theme TOML. `dir` is where a relative font path is looked up.
    pub fn parse(text: &str, dir: Option<PathBuf>) -> Result<Self> {
        let f: ThemeFile = toml::from_str(text).context("theme does not parse")?;
        let c = &f.colors;
        let bytes = if f.font.file == "exo2" {
            EXO2.to_vec()
        } else {
            let p = dir.unwrap_or_default().join(&f.font.file);
            std::fs::read(&p).with_context(|| format!("cannot read font {}", p.display()))?
        };
        let font = |weight: f32| -> Result<FontVec> {
            let mut font = FontVec::try_from_vec(bytes.clone()).map_err(|e| anyhow!("font: {e}"))?;
            // Static fonts have no wght axis; the weight is then whatever the file is.
            font.set_variation(b"wght", weight);
            Ok(font)
        };
        Ok(Self {
            name: f.name.clone(),
            background: hex(&c.background)?,
            accent: hex(&c.accent)?,
            text: hex(&c.text)?,
            dim: hex(&c.dim)?,
            working: hex(&c.working)?,
            untested: hex(&c.untested)?,
            wip: hex(&c.wip)?,
            broken: hex(&c.broken)?,
            title_font: font(f.font.title_weight)?,
            body_font: font(f.font.body_weight)?,
        })
    }
}

/// `#RRGGBB` (the `#` optional).
pub fn hex(s: &str) -> Result<Rgb> {
    let h = s.strip_prefix('#').unwrap_or(s);
    if h.len() != 6 || !h.is_ascii() {
        bail!("colour {s:?} is not #RRGGBB");
    }
    let byte = |i: usize| -> Result<u8> {
        h.get(i..i + 2)
            .and_then(|p| u8::from_str_radix(p, 16).ok())
            .ok_or_else(|| anyhow!("colour {s:?} is not #RRGGBB"))
    };
    Ok([byte(0)?, byte(2)?, byte(4)?])
}

/// `RRGGBB`, lowercase, for shields.io.
pub fn to_hex(c: Rgb) -> String {
    format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_load() {
        for (name, _) in BUILTIN {
            let t = Theme::builtin(name).unwrap();
            assert_eq!(t.name, name);
        }
        assert_eq!(
            Theme::builtin("crimson-desert").unwrap().accent,
            [0xC1, 0x12, 0x1F]
        );
    }

    #[test]
    fn hex_colours() {
        assert_eq!(hex("#1A1315").unwrap(), [0x1A, 0x13, 0x15]);
        assert_eq!(hex("ff7700").unwrap(), [255, 0x77, 0]);
        assert!(hex("#fff").is_err());
        assert!(hex("#gggggg").is_err());
        assert!(hex("#ééé").is_err());
        assert_eq!(to_hex([255, 0x77, 0]), "ff7700");
    }

    #[test]
    fn unknown_theme_is_an_error() {
        assert!(Theme::resolve("solarized", Path::new(".")).is_err());
        assert!(Theme::resolve("missing.toml", Path::new(".")).is_err());
    }
}
