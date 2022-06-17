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
            pname = "videoconverter";
            version = "0.2.1";
            src = lib.cleanSource ./.;

            cargoLock.lockFile = ./Cargo.lock;

            # Point to a nixpkgs ffmpeg rather than using the one on $PATH
            prePatch = ''
              substituteInPlace src/ffmpeg_backend.rs \
                --replace 'const FFMPEG_BIN_PATH: &str = "ffmpeg";'\
                          'const FFMPEG_BIN_PATH: &str = "${ffmpeg}/bin/ffmpeg";'
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
          ffmpeg = pkgs.ffmpeg-full.override {
            nonfreeLicensing = true;
            fdkaacExtlib = true;
          };
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
