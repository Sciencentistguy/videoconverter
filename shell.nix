{ pkgs ? import <nixpkgs> { } }:
with pkgs; mkShell {
  name = "videoconverter";
  buildInputs = [
    llvmPackages_latest.clang
    llvmPackages_latest.lld
    llvmPackages_latest.bintools
    llvmPackages_latest.libclang
    nasm
    rustup
    (ffmpeg-full.override {
      nonfreeLicensing = true;
      fdkaacExtlib = true;
    })
  ];

  # For bindgen
  LIBCLANG_PATH = "${llvmPackages_latest.libclang.lib}/lib";
}
