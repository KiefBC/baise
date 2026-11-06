//! baise: live status badges for game mods.
//!
//! Five bricks, each swappable by configuration:
//!
//! | Brick       | Module      | Answers                                   |
//! | ----------- | ----------- | ----------------------------------------- |
//! | Source      | [`source`]  | what the game is on now                   |
//! | Declaration | [`config`]  | what each mod was verified on             |
//! | Rule        | [`state`]   | how those two become a status             |
//! | Renderer    | [`render`]  | what the picture looks like               |
//! | Publish     | [`outputs`] | the files written for the `badges` branch |
//!
//! Everything except [`source::steam`]'s HTTP call is pure and tested natively.

pub mod clock;
pub mod config;
pub mod outputs;
pub mod releases;
pub mod render;
pub mod source;
pub mod state;
