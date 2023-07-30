{ lib }:

let
  inherit (lib) mkOption types;
in

{
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
}
