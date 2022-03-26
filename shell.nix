{ pkgs ? import <nixpkgs> { } }:
with pkgs; mkShell {
  name = "videoconverter";
  buildInputs = [
    rustup
    clang

    # For testing
    (ffmpeg-full.override {
      nonfreeLicensing = true;
      fdkaacExtlib = true;
    })

    # bindgen
    pkg-config
    rustPlatform.bindgenHook
  ];
}
