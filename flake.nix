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
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = (import nixpkgs) {
          overlays = [ rust-overlay.overlays.default ];
          inherit system;
        };

        inherit (pkgs) callPackage;
        inherit (pkgs.stdenvNoCC.hostPlatform) isDarwin isLinux;

        naersk' = pkgs.callPackage naersk { };

        allPkgs = with pkgs; [ rust-bin.nightly.latest.default rust-analyzer ];
        linuxOnlyPkgs = with pkgs; [ libudev-zero pkg-config ];
        darwinOnlyPkgs = with pkgs; [ darwin.apple_sdk.frameworks.Security ];

      in
      rec {
        # For `nix build` & `nix run`:
        defaultPackage = naersk'.buildPackage {
          src = ./.;
        };

        # For `nix develop`:
        devShell = pkgs.mkShell {
          nativeBuildInputs = allPkgs
            ++ (if isLinux then linuxOnlyPkgs else [ ])
            ++ (if isDarwin then darwinOnlyPkgs else [ ]);
        };
      }
    );
}
