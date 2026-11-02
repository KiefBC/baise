//! The shields.io endpoint schema: `https://img.shields.io/endpoint?url=<raw json url>`.

use serde::Serialize;

use crate::render::theme::to_hex;
use crate::render::Theme;
use crate::state::Status;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub schema_version: u8,
    pub label: String,
    pub message: String,
    pub color: String,
    pub label_color: String,
    pub cache_seconds: u32,
}

/// `Desert Tooling | working · 2.04.05`
pub fn endpoint(label: &str, status: Status, version: Option<&str>, build: &str, theme: &Theme) -> Endpoint {
    let game = version.map_or_else(|| format!("build {build}"), str::to_owned);
    Endpoint {
        schema_version: 1,
        label: label.to_owned(),
        message: format!("{} · {game}", status.short()),
        color: to_hex(theme.status_color(status)),
        label_color: to_hex(theme.background),
        cache_seconds: 300,
    }
}

pub fn to_json(e: &Endpoint) -> String {
    let mut s = serde_json::to_string_pretty(e).unwrap_or_default();
    s.push('\n');
    s
}
