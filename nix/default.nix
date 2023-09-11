# TODO:
# - nixos module
# - home-manager module
# - library for wrapping tools

{
  nixosModule = import ./nixos.nix;
  mkTool =
    { name ? "credible"
    , pkgs
    , storage
    , secrets ? [ ]
    , exposures ? [ ]
    , privateKeyPaths ? [ ]
    , mountPoint ? ""
    , secretDir ? ""
    , owner ? ""
    , group ? ""
    }:
    let
      services = pkgs.callPackage ./services.nix {
        configFiles = [{ inherit secrets storage exposures; }];
        inherit secretDir mountPoint owner group privateKeyPaths;
      };
    in
    pkgs.writeShellScriptBin name ''
      set -euo pipefail
      ${services.exports}

      ${pkgs.credible}/bin/credible "$@"
    '';
}
