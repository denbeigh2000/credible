{ config, options, lib, pkgs, ... }:

let
  inherit (lib)
    callPackage
    concatStringsSep
    mapAttrsToList
    mkEnableOption
    mkIf
    mkOption
    mkPackageOption
    types
    writeShellScriptBin;

  inherit (pkgs.stdenvNoCC.hostPlatform) isDarwin isLinux;

  localTypes = callPackage ./types.nix { };
  inherit (localTypes) secretType;

  cfg = config.credible;

  configFile = {
    inherit (cfg) secrets;
    storage = {
      type = "S3";
      inherit (cfg) bucket;
    };
  };

  services = callPackage ./services.nix {
    configFiles = [ configFile ];
    inherit (cfg) secretDir secretRoot owner group privateKeyPaths;
  };

  wrapped = writeShellScriptBin "credible" ''
    ${services.exports}
    ${cfg.secretSetup}

    ${cfg.package}/bin/credible "$@"
  '';
in
{
  options.credible = {
    enable = mkEnableOption "Manage system secrets with credible";

    package = mkPackageOption pkgs "credible" { };

    secretSetup = mkOption {
      type = types.str;
      default = "";
      description = ''
        Bash script to load any necessary secrets before invoking credible
      '';
    };

    privateKeyPaths = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = ''
        Path to SSH keys to be used when decrypting.
      '';
    };

    ownerGroup = mkOption {
      type = types.str;
      default = "0";
    };

    ownerUser = mkOption {
      type = types.str;
      default = "0";
    };

    secretDir = mkOption {
      type = types.str;
      default = "/run/credible";
    };

    mountDir = mkOption {
      type = types.str;
      default = "/run/credible.d";
    };

    bucket = mkOption {
      type = types.str;
      description = ''
        S3 bucket to retrieve secrets from
      '';
    };

    secrets = mkOption {
      type = types.attrsOf secretType;
      default = { };
      description = ''
        Attrset of secrets.
      '';
    };
  };

  config = mkIf cfg.enable {
    systemd.services.credible = mkIf isLinux services.systemd;
    lauinchd.agents.activate-credible = mkIf isDarwin services.launchd;
  };
}
