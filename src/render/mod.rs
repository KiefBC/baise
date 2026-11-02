//! The Renderer brick. Layouts are Rust modules (only `card` so far); themes
//! are data ([`theme`]).

pub mod canvas;
pub mod card;
pub mod theme;

pub use card::CardData;
pub use theme::Theme;
