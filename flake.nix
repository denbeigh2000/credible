{
  description = "credible: a small tool for managing credentials.";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, flake-utils, naersk, nixpkgs, rust-overlay }:
    let
      nixLibs = import ./nix;
      overlay = import ./overlay.nix { inherit naersk; };
    in
    {
      nixosModules.default = nixLibs.nixosModule;
      lib.wrapTool = nixLibs.mkTool;
      overlays.default = overlay;
    } // flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) {
          overlays = [ rust-overlay.overlays.default overlay ];
          inherit system;
        };
        inherit (pkgs) callPackage rust-bin rust-analyzer;
      in
      rec {
        packages = {
          # For `nix build` & `nix run`:
          inherit (pkgs) credible;
          default = pkgs.credible;
        };

        # For `nix develop`:
        devShell = pkgs.mkShell {
          packages = [ rust-bin.nightly.latest.default rust-analyzer ];
          nativeBuildInputs = callPackage ./systemLibs.nix { };
        };
      }
    );
}
