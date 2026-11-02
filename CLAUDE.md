# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`baise` is a Rust CLI and reusable GitHub Actions workflow that publishes live status badges for
game mods. On a cron, it reads the game's current Steam build, compares it with the build each
mod declares in `baise.toml`, and renders an animated GIF card (plus PNG and shields.io JSON) per
mod. The cards are force-pushed to a `badges` branch and hotlinked from Nexus Mods.

## Commands

All recipes go through `just`. They work both inside `nix develop` and from a plain shell, because
outside the dev shell each command is re-run through `nix develop --command`.

- `just ci`: everything a commit must pass (fmt-check, clippy `-D warnings`, tests, previews, check-pins)
- `just test` runs `cargo test --locked`. For a single test: `cargo test --locked <name>`, or
  `cargo test --test state <name>` for one integration file (`tests/{render,state,steam_fixture}.rs`)
- `just clippy` / `just fmt-check` (`cargo fmt` to fix; `max_width = 110`)
- `just preview` regenerates the 8 cards in `preview/` (2 built-in themes × 4 statuses). CI fails
  if `preview/` differs from the committed files, so run it and commit after any render or theme change.
- `just release` builds the static `x86_64-unknown-linux-musl` binary and asserts it is statically linked
- `just demo 99999999` runs an offline check against `templates/consumer/baise.toml` into
  `target/demo/` (a build other than 26361232 shows UNTESTED)
- `just check-pins` checks that the version pins agree (see Releasing)

## Architecture

`src/lib.rs` documents the five "bricks". Each one is a module and can be swapped through config:

- **Source** (`source/`) tells what build the game is on now. `Source` is a trait. `steam.rs` calls
  api.steamcmd.net for the build id and ISteamNews for the version, which a regex pulls out of
  announcement titles. `manual.rs` is the fixed alternative. Parsing (`parse_build`,
  `parse_version`) is kept apart from fetching, so it can be tested against the recorded
  responses in `tests/fixtures/`. The Steam HTTP call is the only impure part of the crate.
- **Declaration** (`config.rs`) parses and validates `baise.toml`. Each `[[mod]]` gives `key`,
  `build`, `date`, `status` (working|wip|broken) and `on_update` (untested|broken).
- **Rule** (`state.rs`). `resolve()` is pure and checks in this order: a pin (`set` override) for the
  *current* build, then the declared status if `decl.build == current_build`, otherwise `on_update`
  dated from `first_seen[current_build]`. `State` (`state.json`) records the first date each build
  was seen, the pins, the last build and version, and the SYNC counter. It lives on the `badges`
  branch, never on main.
- **Renderer** (`render/`). `canvas.rs` is a small RGBA rasterizer using `ab_glyph`. `card.rs` is
  the only layout so far (960x276, 20 frames, `DELAY_CS`). `theme.rs` loads theme TOML. The built-in
  themes (`themes/*.toml`) and the Exo 2 font are compiled in with `include_str!` and
  `include_bytes!`, so adding a built-in theme means adding it to `BUILTIN` too.
- **Publish** (`outputs/`). `write_mod` renders one card and writes `<key>.gif/.png/.json`. It also
  holds the raw.githubusercontent URL and snippet helpers for `index.md`.

`main.rs` holds the clap CLI with the subcommands `check`, `render`, `set`, `urls` and `preview`.
`check` is the subcommand that does the work: source → `state.observe` → `draw_all` → `state.save`.

**Exit-code contract:** 0 ok, 1 config or state error, 2 source unreachable. Source errors are
wrapped in `Unreachable` so that nothing is written on exit 2, and the workflow then skips the
push. A Steam outage must never blank or falsify a badge, so keep this property when touching
`check`.

## Constraints

- **Shipped code may not panic.** `Cargo.toml` denies `unwrap_used`, `expect_used`,
  `indexing_slicing`, `panic`, `unreachable`, `todo` and `unimplemented`, and forbids `unsafe_code`.
  The binary runs unattended on a cron, and a panic there means a badge that silently stops
  updating. Use `.get()`, `?` and `anyhow` context instead. Tests are exempt (`clippy.toml`).
  Helper code outside `#[test]` fns in `tests/` needs a file-level `#![allow(...)]`.
- `clock.rs` does its own date math (civil-from-days), so there is no chrono or time dependency.
  Keep the dependency set small, because the release is a static musl binary.

## Workflows

- `.github/workflows/status.yml` is the **reusable** workflow that consumers call. It downloads
  the prebuilt release binary (it does not compile anything), runs `check`, or `set` when it gets
  the `mod`/`status` inputs, and force-pushes a single commit to the badges branch.
- `self-status.yml` dogfoods it hourly with `.github/baise.toml` to draw the README's demo card.
  If a release is broken, that card stops updating.
- `templates/consumer/` is the two-file setup for a mod repo. `templates/standalone/` is a public
  status-only repo for mods whose source is private.

## Releasing

Bump `version` in `Cargo.toml`, the `inputs.version` default in `.github/workflows/status.yml`, the
`status.yml@v…` pin in both templates, and the pin in the README install snippet. `just check-pins`
lists any that disagree. Then tag `v<version>`, and `release.yml` attaches the binary.
