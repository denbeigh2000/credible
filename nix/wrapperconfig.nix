{ pkgs, lib, config, ... }:

let
  inherit (lib)
    mkOption
    types;
in

{
  options = {
    storage = mkOption {
      type = types.submodule {
        options = {
          type = mkOption {
            description = "Storage backing";
            type = types.enumOf [ "S3" ];
          };
          bucket = mkOption {
            description = "Bucket in backing storage";
            type = types.str;
          };
          region = mkOption {
            description = "region of bucket";
            type = types.str;
          };
        };
      };

      secrets = mkOption {
        type = types.listOf (types.submodule {
          options = {
            name = mkOption {
              description = "Name of the secret";
              type = types.str;
            };

            encryptionKeys = mkOption {
              description = "SSH public keys to encrypt secret with";
              type = types.listOf types.str;
            };

            path = mkOption {
              description = "Path of key in backing object store";
              type = types.str;
            };
          };
        });
      };
    };

    exposures = mkOption {
      type = types.listOf (types.oneOf [
        (types.submodule {
          options = {
            secretName = mkOption {
              description = "Name of secret to expose";
              type = types.str;
            };

            type = mkOption {
              description = ''Type of exposure (must be "env")'';
              type = types.enumOf [ "env" ];
            };

            name = mkOption {
              description = "Environment variable name to set";
              type = types.str;
            };
          };
        })

        (types.submodule {
          options = {
            secretName = mkOption {
              description = "Name of secret to expose";
              type = types.str;
            };

            type = mkOption {
              description = ''Type of exposure (must be "file")'';
              type = types.enumOf [ "file" ];
            };

            path = mkOption {
              description = "Path to file expose secret within";
              type = types.str;
            };
          };
        })
      ]);
    };
  };
}
