{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };
  outputs = {
    self,
    nixpkgs,
    flake-utils,
    ...
  }:
    {
      overlay = final: prev: {
        videoconverter = self.packages.${prev.system}.default;
      };
    }
    // flake-utils.lib.eachDefaultSystem (system:
      # Instantiating a nixpkgs here as it needs to have `config.allowUnfree = true;`.
      # Using https://github.com/numtide/nixpkgs-unfree caused opencv to build from source.
      let
        pkgs = import "${nixpkgs}" {
          config.allowUnfree = true;
          inherit system;
        };
        inherit (pkgs) lib rustPlatform;
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

            # Point to a nixpkgs ffmpeg rather than using the one on $PATH
            prePatch = let
              nnedi_weights = builtins.fetchurl {
                url = "https://github.com/dubhater/vapoursynth-nnedi3/raw/cc6f6065e09c9241553cb51f10002a7314d66bfa/src/nnedi3_weights.bin";
                sha256 = "0hhx4n19qaj3g68f5kqjk23cj063g4y2zidivq9pdfrm0i1q5wr7";
              };
            in ''
              substituteInPlace src/ffmpeg_backend.rs \
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

            buildInputs = [
              ffmpeg
            ];

            meta = with lib; {
              license = licenses.mpl20;
              homepage = "https://github.com/Sciencentistguy/videoconverter";
              platforms = ffmpeg.meta.platforms;
            };
          };
      in {
        packages.videoconverter = pkgs.callPackage videoconverter {
          ffmpeg = pkgs.ffmpeg_5-full.override {
            nonfreeLicensing = true;
            fdkaacExtlib = true;
          };
        };
        # Assume that ffmpeg-full works and I don't need to build it in CI
        packages.videoconverter-ci = pkgs.callPackage videoconverter {
          ffmpeg = pkgs.ffmpeg_5;
        };

        packages.default = self.packages.${system}.videoconverter;

        devShells.default = self.packages.${system}.default.overrideAttrs (super: {
          nativeBuildInputs = with pkgs;
            super.nativeBuildInputs
            ++ [
              clippy
              rustfmt
              cargo-edit
            ];
          RUST_SRC_PATH = "${rustPlatform.rustLibSrc}";
        });
      });
}
