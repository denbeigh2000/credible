{ config, options, lib, pkgs, ... }:

let
  inherit (lib) callPackage concatStringsSep mapAttrsToList mkOption mkPackageOption types
    writeShellScriptBin;

  cfg = config.credible;

  configFile = writeText "credible.json" (builtins.toJSON {
    inherit (cfg) secrets;
    backingConfig = {
      type = "S3";
      inherit (cfg) bucket;
    };
  });

  services = callPackage ./services.nix {
    inherit configFile;
    inherit (cfg) secretDir secretRoot owner group privateKeyPaths;
  };

  wrapped = writeShellScriptBin "credible" ''
    ${services.exports}

    ${credible}/bin/credible "$@"
  '';


  secretType = types.submodule ({ config, ... }: {
    options = {
      name = mkOption {
        type = types.str;
        default = config._module.args.name;
        defaultText = literalExpression "config._module.args.name";
        description = ''
          Name of the file used in {option}`age.secretsDir`
        '';
      };
      path = mkOption {
        type = types.str;
        description = ''
          Remote storage path to the secret.
        '';
      };
      mountPath = mkOption {
        type = types.str;
        default = "${cfg.secretsDir}/${config.name}";
        defaultText = literalExpression ''
          "''${cfg.secretsDir}/''${config.name}"
        '';
        description = ''
          Additional vanity symlink for the decrypted secret.
        '';
      };
      encryptionKeys = mkOption {
        type = types.listOf types.str;
      };
      user = mkOption {
        type = types.str;
        default = "0";
        description = ''
          User of the decrypted secret.
        '';
      };
      group = mkOption {
        type = types.str;
        default = users.${config.owner}.group or "0";
        defaultText = literalExpression ''
          users.''${config.owner}.group or "0"
        '';
        description = ''
          Group of the decrypted secret.
        '';
      };
    };
  });
in

{
  options.credible = {
    package = mkPackageOption pkgs "credible" { };

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

  config = { };
}
