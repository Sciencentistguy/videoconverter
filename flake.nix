{
  inputs = {
    nixpkgs = {
      url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    };
    nixpkgs-unfree = {
      url = "github:numtide/nixpkgs-unfree";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
    flake-utils = {
      url = "github:numtide/flake-utils";
    };
  };
  outputs = {
    self,
    nixpkgs-unfree,
    flake-utils,
    ...
  }:
    {
      overlay = final: prev: {
        videoconverter = self.packages.${prev.system}.default;
      };
    }
    // flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs-unfree.legacyPackages.${system};
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

          prePatch = ''
            substituteInPlace src/backend.rs --replace 'const FFMPEG_BIN_PATH: &str = "ffmpeg";' 'const FFMPEG_BIN_PATH: &str = "${ffmpeg}/bin/ffmpeg";'
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

      devShells.default = self.packages.${system}.videoconverter.overrideAttrs (super: {
        nativeBuildInputs = with pkgs;
          super.nativeBuildInputs
          ++ [
            clippy
            rustfmt
            cargo-edit
          ];
        RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
      });
    });
}
