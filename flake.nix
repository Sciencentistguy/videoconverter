{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
    fenix.url = "github:nix-community/fenix";
  };
  outputs = { self, nixpkgs, flake-utils, fenix, ... }:
    {
      overlay = final: prev: {
        videoconverter = self.packages.${prev.system}.default;
      };
    }
    // flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          config.allowUnfree = true;
          inherit system;
        };
        inherit (pkgs) lib;
        fenixStable = fenix.packages.${system}.stable;
        rustToolchain = fenixStable.toolchain;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain.cargo;
          rustc = rustToolchain.rustc;
        };

        nnedi_weights = pkgs.fetchurl {
          url = "https://github.com/dubhater/vapoursynth-nnedi3/raw/cc6f6065e09c9241553cb51f10002a7314d66bfa/src/nnedi3_weights.bin";
          sha256 = "0hhx4n19qaj3g68f5kqjk23cj063g4y2zidivq9pdfrm0i1q5wr7";
        };

        videoconverter = {
          rustPlatform,
          lib,
          pkg-config,
          ffmpeg,
        }:
          rustPlatform.buildRustPackage {
            name = "videoconverter";
            src = lib.cleanSource ./.;

            cargoLock.lockFile = ./Cargo.lock;

            prePatch = ''
              substituteInPlace src/command.rs \
                --replace 'const FFMPEG_BIN_PATH: &str = "ffmpeg";'\
                          'const FFMPEG_BIN_PATH: &str = "${ffmpeg}/bin/ffmpeg";'

              substituteInPlace src/interface.rs \
                --replace '"~/.ffmpeg/nnedi3_weights"'\
                          '"${nnedi_weights}"'
            '';

            nativeBuildInputs = [
              pkg-config
              rustPlatform.bindgenHook
            ];

            buildInputs = with pkgs; (
              [
                ffmpeg
              ]
              ++ lib.optionals (stdenv.isDarwin) [
                libiconv
              ]
            );

            inherit ffmpeg;

            meta = with lib; {
              license = licenses.mpl20;
              homepage = "https://github.com/Sciencentistguy/videoconverter";
              platforms = ffmpeg.meta.platforms;
            };
          };
      in {
        packages.videoconverter = pkgs.callPackage videoconverter {
          ffmpeg = pkgs.ffmpeg_7.override {
            ffmpegVariant = "full";
            withFullDeps = true;
            withUnfree = true;
          };
        };
        packages.videoconverter-ci = pkgs.callPackage videoconverter {
          ffmpeg = pkgs.ffmpeg_7;
        };

        packages.nnedi_weights = nnedi_weights;

        packages.default = self.packages.${system}.videoconverter;

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            fenixStable.rustfmt
            fenixStable.clippy
            pkgs.cargo-edit
            pkgs.pkg-config
            pkgs.ffmpeg_7
            pkgs.libiconv
          ];
          RUST_SRC_PATH = "${fenixStable.rust-src}/lib/rustlib/src/rust/library";
        };
      });
}
