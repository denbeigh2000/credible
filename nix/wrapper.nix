pkgs:
{ config
, privateKeyPaths ? [ ]
, owner ? ""
, group ? owner
, mountPoint ? ""
, secretDir ? ""
, name ? "credible"
}:

let
  inherit (pkgs) writeText writeShellScriptBin;
  inherit (pkgs.lib) concatStringsSep;

  libshell = pkgs.callPackage ./libshell.nix { };

  configFile = writeText "credible.json" (builtins.toJSON config);
  environment = {
    CREDIBLE_CONFIG_FILES = configFile;
    CREDIBLE_MOUNT_POINT = mountPoint;
    CREDIBLE_SECRET_DIR = secretDir;
    CREDIBLE_OWNER_USER = owner;
    CREDIBLE_OWNER_GROUP = group;
    CREDIBLE_PRIVATE_KEY_PATHS = concatStringsSep "," privateKeyPaths;
  };
in
writeShellScriptBin name ''
  set -euo pipefail
  ${libshell.mkExports environment}

  exec ${pkgs.credible}/bin/credible "$@"
''




