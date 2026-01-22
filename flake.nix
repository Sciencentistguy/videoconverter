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
  outputs = {
    self,
    nixpkgs,
    flake-utils,
    fenix,
    ...
  }:
    {
      overlay = final: prev: {
        videoconverter = self.packages.${prev.system}.default;
      };
    }
    // flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        config.allowUnfree = true;
        inherit system;
      };
      inherit (pkgs) lib;
      fenixStable = fenix.packages.${system}.stable;
      rustToolchain = fenixStable.toolchain;
      rustPlatform = pkgs.makeRustPlatform {
        cargo = rustToolchain;
        rustc = rustToolchain;
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
        sqlite,
      }:
        rustPlatform.buildRustPackage {
          name = "videoconverter";
          src = lib.cleanSource ./.;

          cargoLock.lockFile = ./Cargo.lock;

          prePatch = ''
            substituteInPlace src/interface.rs \
              --replace-warn 'const FFMPEG_BIN_PATH: &str = "ffmpeg";'\
                        'const FFMPEG_BIN_PATH: &str = "${ffmpeg}/bin/ffmpeg";' \
              --replace-warn 'const NNEDI_WEIGHTS_PATH: &str = "~/.ffmpeg/nnedi3_weights.bin";'\
                        'const NNEDI_WEIGHTS_PATH: &str = "${nnedi_weights}";' \
          '';

          nativeBuildInputs = [
            pkg-config
            rustPlatform.bindgenHook
          ];

          buildInputs = with pkgs; (
            [
              ffmpeg
              sqlite
              rustPlatform.bindgenHook
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
    in rec {
      packages.videoconverter = pkgs.callPackage videoconverter {
        ffmpeg = pkgs.ffmpeg_7.override {
          ffmpegVariant = "full";
          withFullDeps = true;
          withUnfree = true;
        };
        inherit rustPlatform;
      };
      packages.videoconverter-ci = pkgs.callPackage videoconverter {
        ffmpeg = pkgs.ffmpeg_7;
        inherit rustPlatform;
      };

      packages.nnedi_weights = nnedi_weights;

      packages.default = self.packages.${system}.videoconverter;

      devShells.default = pkgs.mkShell {
        inputsFrom = [
          packages.videoconverter-ci
        ];
        RUST_SRC_PATH = "${fenixStable.rust-src}/lib/rustlib/src/rust/library";
      };
    });
}
