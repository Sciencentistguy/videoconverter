{
  inputs = {
    # github example, also supported gitlab:
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
      # AFAICT I have to instantiate a nixpkgs here because of the unfree, even though it is le bad
      let
        pkgs = import "${nixpkgs}" {
          config.allowUnfree = true;
          inherit system;
        };
        inherit (pkgs) lib rustPlatform;
      in {
        packages.default = let
          ffmpeg-fdk = pkgs.ffmpeg-full.override {
            nonfreeLicensing = true;
            fdkaacExtlib = true;
          };
        in
          rustPlatform.buildRustPackage {
            pname = "videoconverter";
            version = "0.2.1";
            src = lib.cleanSource ./.;

            cargoLock.lockFile = ./Cargo.lock;

            prePatch = ''
              substituteInPlace src/backend.rs --replace 'const FFMPEG_BIN_PATH: &str = "ffmpeg";' 'const FFMPEG_BIN_PATH: &str = "${ffmpeg-fdk}/bin/ffmpeg";'
            '';

            nativeBuildInputs = with pkgs; [
              pkg-config
              rustPlatform.bindgenHook
              clippy
            ];

            buildInputs = [
              ffmpeg-fdk
            ];

            RUST_SRC_PATH = "${rustPlatform.rustLibSrc}";
          };
      });
}
