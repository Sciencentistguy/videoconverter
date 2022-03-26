{ ffmpeg-full
, lib
, llvm
, pkg-config
, rustPlatform
}:

let
  ffmpeg = (ffmpeg-full.override {
    nonfreeLicensing = true;
    fdkaacExtlib = true;
  });
in
rustPlatform.buildRustPackage {
  pname = "videoconverter";
  version = "0.2.1";
  src = lib.cleanSource ./.;

  cargoLock.lockFile = ./Cargo.lock;

  prePatch = ''
    substituteInPlace src/backend.rs --replace 'const FFMPEG_BIN_PATH: &str = "ffmpeg";' 'const FFMPEG_BIN_PATH: &str = "${ffmpeg}/bin/ffmpeg";'
  '';

  nativeBuildInputs = [
    llvm
    pkg-config
    rustPlatform.bindgenHook
  ];

  buildInputs = [
    ffmpeg
  ];
}
