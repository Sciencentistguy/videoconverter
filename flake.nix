{
  inputs = {
    # github example, also supported gitlab:
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    flake-compat = {
      url = github:edolstra/flake-compat;
      flake = false;
    };
  };
  outputs = { self, nixpkgs, flake-utils, ... }: {
    overlay = final: prev:
      let
        pkgs = import "${nixpkgs}" {
          config = { allowUnfree = true; };
          system = prev.system;
        };
      in
      {
        videoconverter = pkgs.callPackage ./videoconverter.nix { };
      };
  } //
  flake-utils.lib.eachDefaultSystem (system:
    # AFAICT I have to instantiate a nixpkgs here because of the unfree, even though it is le bad
    let pkgs = import "${nixpkgs}" {
      config = { allowUnfree = true; };
      inherit system;
    };
    in
    {
      packages.default = pkgs.callPackage ./videoconverter.nix { };
    });
}
