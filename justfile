# mod-status - task runner. `just` lists the recipes.
#
# Every recipe works inside `nix develop` and from a plain shell: outside the
# dev shell each command is re-run through `nix develop --command`.

set shell := ["bash", "-euo", "pipefail", "-c"]

export NIX_CONFIG := "experimental-features = nix-command flakes"

# Empty inside the dev shell, the nix wrapper outside it.
nix := if env("IN_NIX_SHELL", "") == "" { "nix develop --command" } else { "" }

musl := "x86_64-unknown-linux-musl"

# List the recipes.
default:
    @just --list --unsorted

# Debug build of the CLI for this machine.
build:
    {{nix}} cargo build

# The release binary: static x86_64-linux-musl, what release.yml uploads.
release:
    {{nix}} cargo build --release --locked --target {{musl}}
    {{nix}} file target/{{musl}}/release/mod-status | tee /dev/stderr | grep -Eq 'static(ally|-pie) linked'

# Unit and integration tests (native).
test:
    {{nix}} cargo test --locked

# Clippy; any warning fails.
clippy:
    {{nix}} cargo clippy --locked --all-targets -- -D warnings

# Formatting check (`cargo fmt` to fix).
fmt-check:
    {{nix}} cargo fmt --check

# Every status in every built-in theme into preview/ (8 GIFs, 8 PNGs, 8 JSONs).
preview:
    {{nix}} cargo run --release --locked --quiet -- preview --out preview
    test "$(ls preview/*.gif | wc -l)" -eq 8

# Check Cargo.lock against the RustSec advisory database (needs the network).
audit:
    {{nix}} cargo audit

# Everything a commit should pass.
ci: fmt-check clippy test preview check-pins

# Offline run against the consumer template: `just demo 99999999` shows UNTESTED.
demo build="26361232":
    rm -rf target/demo && mkdir -p target/demo
    {{nix}} cargo run --release --locked --quiet -- check --offline --build {{build}} --game-version 2.04.05 \
        --config templates/consumer/mod-status.toml --state target/demo/state.json --out target/demo \
        --repo KiefBC/desert-modding

# The reusable workflow's default release and the templates' @ref match Cargo.toml's version.
check-pins:
    #!/usr/bin/env bash
    set -euo pipefail
    v="v$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
    fail=0
    grep -A3 '^      version:' .github/workflows/status.yml | grep -q "default: $v" \
        || { echo "status.yml: inputs.version default is not $v"; fail=1; }
    for f in templates/consumer/status.yml templates/standalone/.github/workflows/status.yml; do
        grep -q "status.yml@$v\$" "$f" || { echo "$f: not pinned to @$v"; fail=1; }
    done
    grep -q "status.yml@$v" README.md || { echo "README.md: install snippet not pinned to @$v"; fail=1; }
    exit $fail
