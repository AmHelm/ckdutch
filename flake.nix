{
  description = "Development environment for the 2026 Offsite Hackathon quickstart";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";

    cargo-near-src = {
      url = "github:near/cargo-near";
      flake = false;
    };
    near-cli-src = {
      url = "github:near/near-cli-rs";
      flake = false;
    };
    near-mpc-src = {
      url = "github:near/mpc/3.14.0";
      flake = false;
    };
  };

  outputs = { nixpkgs, flake-utils, fenix, cargo-near-src, near-cli-src, near-mpc-src, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        toolchain = fenix.packages.${system}.combine [
          (fenix.packages.${system}.stable.withComponents [
            "cargo"
            "clippy"
            "rust-src"
            "rustc"
            "rustfmt"
            "rust-analyzer"
          ])
          fenix.packages.${system}.targets.wasm32-unknown-unknown.stable.rust-std
        ];
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };

        cargo-near = rustPlatform.buildRustPackage {
          pname = "cargo-near";
          version = "source";
          src = cargo-near-src;
          cargoLock.lockFile = "${cargo-near-src}/Cargo.lock";
          cargoBuildFlags = [ "-p" "cargo-near" ];
          buildNoDefaultFeatures = true;
          doCheck = false;
          nativeBuildInputs = [ pkgs.autoPatchelfHook pkgs.perl pkgs.pkg-config ];
          buildInputs = [ pkgs.stdenv.cc.cc.lib ];
        };

        near-cli = rustPlatform.buildRustPackage {
          pname = "near-cli-rs";
          version = "source";
          src = near-cli-src;
          cargoLock.lockFile = "${near-cli-src}/Cargo.lock";
          buildNoDefaultFeatures = true;
          doCheck = false;
          nativeBuildInputs = [ pkgs.autoPatchelfHook pkgs.perl pkgs.pkg-config ];
          buildInputs = [ pkgs.stdenv.cc.cc.lib ];
        };

        ckd-example-cli = rustPlatform.buildRustPackage {
          pname = "ckd-example-cli";
          version = "3.14.0";
          src = near-mpc-src;
          cargoLock = {
            lockFile = "${near-mpc-src}/Cargo.lock";
            outputHashes = {
              "near-async-2.13.3" = "sha256-CTxGy2iP1dkpB42cpyRboGWMLqjQ9zWVYAfrVm+NlrA=";
            };
          };
          cargoBuildFlags = [ "-p" "ckd-example-cli" ];
          doCheck = false;
        };
      in {
        packages = {
          inherit cargo-near near-cli ckd-example-cli;
          default = near-cli;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            cargo-near
            near-cli
            ckd-example-cli
            pkgs.openssl
            pkgs.trunk
          ];
        };
      });
}
