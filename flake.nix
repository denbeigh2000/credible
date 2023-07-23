{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, flake-utils, naersk, nixpkgs, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) {
          overlays = [ rust-overlay.overlays.default ];
          inherit system;
        };

        inherit (pkgs) callPackage;
        inherit (pkgs.stdenvNoCC.hostPlatform) isLinux;

        naersk' = pkgs.callPackage naersk { };

        linuxOnlyPkgs = with pkgs; [ libudev-zero pkg-config ];

      in
      rec {
        # For `nix build` & `nix run`:
        defaultPackage = naersk'.buildPackage {
          src = ./.;
        };

        # For `nix develop`:
        devShell = pkgs.mkShell {
          nativeBuildInputs = with pkgs; [
            rust-bin.nightly.latest.default
            rust-analyzer
          ] ++ (if isLinux then linuxOnlyPkgs else []);
        };
      }
    );
}
