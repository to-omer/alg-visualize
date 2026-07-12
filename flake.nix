{
  description = "Reproducible toolchain for the ordered-map algorithm visualizer";

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
        "aarch64-darwin"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      devShells = forAllSystems (
        system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          rust = pkgs.rust-bin.stable."1.94.0".default.override {
            extensions = [
              "clippy"
              "rust-src"
              "rustfmt"
            ];
            targets = [ "wasm32-unknown-unknown" ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              pkgs.binaryen
              pkgs.cargo-deny
              pkgs.cargo-nextest
              pkgs.git
              pkgs.jq
              pkgs.just
              pkgs.nodejs_24
              pkgs.nixfmt
              pkgs.pnpm_11
              pkgs.wasm-bindgen-cli
              pkgs.wasm-tools
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ pkgs.playwright-driver.browsers ];

            RUST_BACKTRACE = "1";
            SOURCE_DATE_EPOCH = "1";
            PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
            shellHook = pkgs.lib.optionalString pkgs.stdenv.isLinux ''
              export PLAYWRIGHT_BROWSERS_PATH=${pkgs.playwright-driver.browsers}
            '';
          };
        }
      );

      checks = forAllSystems (
        system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          rust = pkgs.rust-bin.stable."1.94.0".default.override {
            extensions = [
              "clippy"
              "rustfmt"
            ];
            targets = [ "wasm32-unknown-unknown" ];
          };
        in
        assert pkgs.playwright-driver.version == "1.61.1";
        {
          toolchain =
            pkgs.runCommand "alg-visualize-toolchain"
              {
                nativeBuildInputs = [
                  rust
                  pkgs.nodejs_24
                  pkgs.pnpm_11
                  pkgs.wasm-bindgen-cli
                ];
              }
              ''
                rustc --version | grep -F 'rustc 1.94.0'
                node --version | grep -F 'v24.'
                pnpm --version | grep -F '11.9.0'
                wasm-bindgen --version | grep -F '0.2.121'
                touch "$out"
              '';
        }
      );

      formatter = forAllSystems (
        system:
        let
          pkgs = import nixpkgs { inherit system; };
        in
        pkgs.nixfmt
      );
    };
}
