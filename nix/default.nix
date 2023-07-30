# TODO:
# - nixos module
# - home-manager module
# - library for wrapping tools

{
  nixosModule = import ./nixos.nix;
  mkTool =
    { name ? "credible"
    , pkgs
    , secrets
    , storage
    , privateKeyPaths ? []
    , mountPoint ? ""
    , secretDir ? ""
    , owner ? ""
    , group ? ""
    }:
    let
      services = pkgs.callPackage ./services.nix {
        configFile = { inherit secrets storage; };
        inherit secretDir mountPoint owner group privateKeyPaths;
      };
    in
    pkgs.writeShellScriptBin name ''
      set -euo pipefail
      ${services.exports};

      ${pkgs.credible}/bin/credible "$@"
    '';
}
