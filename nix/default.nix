# TODO:
# - nixos module
# - home-manager module
# - library for wrapping tools

{
  nixosModule = import ./nixos.nix;
  mkTool =
    { pkgs
    , secrets
    , backingConfig
    , secretDir
    , secretRoot
    , user
    , group
    , privateKeyPaths
    }:
    let
      services = pkgs.callPackage ./services.nix {
        configFile = { inherit secrets backingConfig; };
        inherit secretDir secretRoot user group privateKeyPaths;
      };
    in
    writeShellScriptBin "with-secrets" ''
      set -euo pipefail
      ${services.exports};

      ${pkgs.credible}/bin/credible run-command -- "$@"
    '';
}
