{
  description = "baise - live status badges for game mods (Rust; static x86_64-linux-musl release binary)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        # Stable Rust, plus the musl target the release binary is built for.
        toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
          targets = [ "x86_64-unknown-linux-musl" ];
        };

        # ring (rustls's crypto) compiles C and assembly, so the musl target
        # needs a musl C compiler; it doubles as the linker.
        musl = pkgs.pkgsCross.musl64.stdenv.cc;
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            musl
            pkgs.just
            pkgs.cargo-audit
            pkgs.file    # `just release` checks the binary is statically linked
          ];

          CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${musl}/bin/x86_64-unknown-linux-musl-cc";
          CC_x86_64_unknown_linux_musl = "${musl}/bin/x86_64-unknown-linux-musl-cc";
          AR_x86_64_unknown_linux_musl = "${musl}/bin/x86_64-unknown-linux-musl-ar";

          shellHook = ''
            echo "baise dev shell (just lists the recipes)."
          '';
        };
      });
}
