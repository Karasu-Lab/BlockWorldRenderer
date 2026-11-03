{
  description = "Block world renderer written in Rust, built to WebAssembly";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, rust-overlay, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          # Pinned to the nightly in third_party/Client/rust-toolchain.toml;
          # bump it together with that file.
          rustToolchain = pkgs.rust-bin.nightly."2026-08-21".default.override {
            extensions = [
              "rustfmt"
              "clippy"
              "rust-analyzer"
              "rust-src"
            ];
            targets = [ "wasm32-unknown-unknown" ];
          };
          nativeLibs = pkgs.lib.optionals pkgs.stdenv.isLinux (
            with pkgs;
            [
              vulkan-loader
              wayland
              libxkbcommon
              libx11
              libxcursor
              libxi
              libxrandr
              libxcb
              alsa-lib
              udev
              dbus
              openssl
            ]
          );
        in
        {
          default = pkgs.mkShell (
            {
              packages = [
                rustToolchain
                # Version must match the wasm-bindgen crate in Cargo.toml,
                # which is pinned for this CLI.
                pkgs.wasm-bindgen-cli_0_2_129
                pkgs.just
                pkgs.cargo-nextest
                pkgs.pkg-config
                pkgs.cmake
                pkgs.ninja
                pkgs.python3
                pkgs.nodejs
                pkgs.pnpm
                pkgs.vulkan-headers
              ]
              ++ nativeLibs;
            }
            // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
              LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath nativeLibs;
              RUSTFLAGS = "-L native=${pkgs.vulkan-loader}/lib";
            }
          );
        }
      );
    };
}
