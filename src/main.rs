//! `baise`: check | render | set | urls | preview.
//!
//! Exit codes: 0 ok, 1 config or state error, 2 the source was unreachable
//! (nothing is written, so the last published badge stays up).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use baise::clock::Timestamp;
use baise::config::Config;
use baise::outputs;
use baise::render::{CardData, Theme};
use baise::source::{self, manual::Manual, GameVersion, Source};
use baise::state::{resolve, Origin, State, Status};

#[derive(Parser)]
#[command(name = "baise", version, about = "Live status badges for game mods")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Args)]
struct Common {
    /// The mod repo's declaration file.
    #[arg(long, default_value = "baise.toml")]
    config: PathBuf,
    /// The kit's memory between runs (lives on the badges branch).
    #[arg(long, default_value = "badges/state.json")]
    state: PathBuf,
    /// Where the images, endpoint JSON and index.md go.
    #[arg(long, default_value = "badges")]
    out: PathBuf,
    /// `owner/name`: also write index.md with the hotlink URLs.
    #[arg(long)]
    repo: Option<String>,
    /// The branch the outputs are published to (for index.md).
    #[arg(long, default_value = "badges")]
    branch: String,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read the game's version from the source, apply the rule, draw every card.
    Check {
        #[command(flatten)]
        common: Common,
        /// The SYNC number to show (the workflow passes its run number); default: previous + 1.
        #[arg(long)]
        sync: Option<u64>,
        /// Do not touch the network; take the build from --build.
        #[arg(long, requires = "build")]
        offline: bool,
        /// With --offline: the game build to pretend Steam reports.
        #[arg(long)]
        build: Option<String>,
        /// With --offline: the patch version to show.
        #[arg(long = "game-version")]
        game_version: Option<String>,
    },
    /// Re-draw every card from state.json, without the network.
    Render {
        #[command(flatten)]
        common: Common,
    },
    /// Pin a status for one mod on the current build (`auto` removes the pin).
    Set {
        #[arg(long, default_value = "baise.toml")]
        config: PathBuf,
        #[arg(long, default_value = "badges/state.json")]
        state: PathBuf,
        /// The mod's key.
        key: String,
        /// working | broken | wip | untested | auto
        status: String,
        /// The build to pin to; default: the last build `check` saw.
        #[arg(long)]
        build: Option<String>,
    },
    /// Print the BBCode and Markdown to paste, per mod.
    Urls {
        #[arg(long, default_value = "baise.toml")]
        config: PathBuf,
        /// `owner/name` of the repo the badges branch lives in.
        #[arg(long)]
        repo: String,
        #[arg(long, default_value = "badges")]
        branch: String,
    },
    /// Render every status in every built-in theme into a directory.
    Preview {
        #[arg(long, default_value = "preview")]
        out: PathBuf,
    },
}

/// An error that maps to exit code 2.
#[derive(Debug)]
struct Unreachable(anyhow::Error);

impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source unreachable, nothing written: {:#}", self.0)
    }
}
impl std::error::Error for Unreachable {}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.cmd) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is::<Unreachable>() => {
            eprintln!("baise: {e}");
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("baise: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn run(cmd: Cmd) -> Result<()> {
    let now = Timestamp::now();
    match cmd {
        Cmd::Check {
            common,
            sync,
            offline,
            build,
            game_version,
        } => {
            let cfg = Config::load(&common.config)?;
            let mut state = State::load(&common.state)?;
            let src: Box<dyn Source> = match (offline, build) {
                (true, Some(build)) => Box::new(Manual {
                    build,
                    version: game_version,
                }),
                (true, None) => bail!("--offline needs --build"),
                (false, Some(_)) => bail!("--build only makes sense with --offline"),
                (false, None) => source::from_config(&cfg.source)?,
            };
            let current = src.current().map_err(Unreachable)?;
            state.observe(&current, now, sync);
            draw_all(&cfg, &common, &state, &current)?;
            state.save(&common.state)?;
            Ok(())
        }
        Cmd::Render { common } => {
            let cfg = Config::load(&common.config)?;
            let state = State::load(&common.state)?;
            let current = state.current()?;
            draw_all(&cfg, &common, &state, &current)
        }
        Cmd::Set {
            config,
            state: state_path,
            key,
            status,
            build,
        } => {
            let cfg = Config::load(&config)?;
            if cfg.find(&key).is_none() {
                let keys: Vec<&str> = cfg.mods.iter().map(|m| m.key.as_str()).collect();
                bail!(
                    "no mod {key:?} in {} (have: {})",
                    config.display(),
                    keys.join(", ")
                );
            }
            let status = match status.as_str() {
                "auto" | "clear" => None,
                s => Some(s.parse::<Status>()?),
            };
            let mut state = State::load(&state_path)?;
            state.set_override(&key, status, build.as_deref(), now)?;
            if let Some(parent) = state_path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            state.save(&state_path)?;
            match status {
                Some(s) => println!(
                    "{key}: pinned to {s} on build {}",
                    state.overrides.get(&key).map_or("?", |o| o.build.as_str())
                ),
                None => println!("{key}: pin removed, the rule decides again"),
            }
            Ok(())
        }
        Cmd::Urls { config, repo, branch } => {
            let cfg = Config::load(&config)?;
            check_repo(&repo)?;
            for m in &cfg.mods {
                println!("{}", outputs::snippets(&repo, &branch, &m.key, &m.label));
            }
            Ok(())
        }
        Cmd::Preview { out } => preview(&out),
    }
}

fn check_repo(repo: &str) -> Result<()> {
    let ok = repo
        .split_once('/')
        .is_some_and(|(o, n)| !o.is_empty() && !n.is_empty() && !n.contains('/'));
    if !ok {
        bail!("--repo {repo:?}: expected owner/name");
    }
    Ok(())
}

fn config_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
}

fn draw_all(cfg: &Config, common: &Common, state: &State, current: &GameVersion) -> Result<()> {
    let theme = Theme::resolve(&cfg.render.theme, config_dir(&common.config))?;
    std::fs::create_dir_all(&common.out)
        .with_context(|| format!("cannot create {}", common.out.display()))?;
    let last_sync = state.last_sync().map(Timestamp::datetime).unwrap_or_default();
    println!(
        "game build {}{}, sync #{}",
        current.build,
        current
            .version
            .as_deref()
            .map(|v| format!(" (version {v})"))
            .unwrap_or_default(),
        state.sync
    );
    for decl in &cfg.mods {
        let r = resolve(decl, &current.build, state);
        let data = CardData {
            title_prefix: cfg.render.title_prefix.clone(),
            label: r.label.clone(),
            status: r.status,
            since: r.since.clone(),
            note: r.note.clone(),
            build: current.build.clone(),
            version: current.version.clone(),
            verified_build: decl.build.clone(),
            sync: state.sync,
            last_sync: last_sync.clone(),
        };
        let w = outputs::write_mod(&common.out, &decl.key, &data, &theme)?;
        let why = match r.origin {
            Origin::Declared => format!("declared for build {}", decl.build),
            Origin::GameMoved => format!("game moved from build {}", decl.build),
            Origin::Override => "pinned with `set`".to_owned(),
        };
        println!(
            "  {:<24} {:<24} since {}  ({why}; gif {} KB)",
            decl.key,
            r.status.headline(),
            r.since,
            w.gif_bytes / 1024
        );
    }
    if let Some(repo) = &common.repo {
        check_repo(repo)?;
        let mods: Vec<(&str, &str)> = cfg
            .mods
            .iter()
            .map(|m| (m.key.as_str(), m.label.as_str()))
            .collect();
        let p = common.out.join("index.md");
        std::fs::write(&p, outputs::index_md(repo, &common.branch, &mods))
            .with_context(|| format!("cannot write {}", p.display()))?;
    }
    Ok(())
}

fn preview(out: &Path) -> Result<()> {
    std::fs::create_dir_all(out).with_context(|| format!("cannot create {}", out.display()))?;
    for (name, _) in baise::render::theme::BUILTIN {
        let theme = Theme::builtin(name)?;
        for status in Status::ALL {
            let moved = status == Status::Untested;
            let data = CardData {
                title_prefix: "LIVE STATUS // AUTO-CHECKED".into(),
                label: "Desert Tooling".into(),
                status,
                since: if moved {
                    "2026-11-02".into()
                } else {
                    "2026-11-01".into()
                },
                note: if status == Status::Wip {
                    "re-targeting signatures".into()
                } else {
                    String::new()
                },
                build: if moved {
                    "26370001".into()
                } else {
                    "26361232".into()
                },
                version: Some(if moved { "2.04.06" } else { "2.04.05" }.into()),
                verified_build: "26361232".into(),
                sync: 4242,
                last_sync: "2026-11-02 04:15 UTC".into(),
            };
            let key = format!("{name}-{status}");
            let w = outputs::write_mod(out, &key, &data, &theme)?;
            println!("{}/{key}.gif  ({} KB)", out.display(), w.gif_bytes / 1024);
        }
    }
    Ok(())
}
