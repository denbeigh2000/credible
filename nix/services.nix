{ lib
, credible
, writeText
, configFiles
, secretDir
, mountPoint
, owner
, group
, exposureConfigs
, mountConfigPaths
, privateKeyPaths
, gnugrep
, coreutils
}:

let
  inherit (lib) mapAttrsToList concatStringsSep optionalString;
  inherit (builtins) map;

  writeFile = f: writeText "credible.json" (builtins.toJSON f);

  mountScript = ''
    ${credible}/bin/credible system mount
  '';

  writtenConfigFiles = map writeFile configFiles;

  commaJoin = things: concatStringsSep "," things;

  environment = {
    CREDIBLE_CONFIG_FILES = commaJoin (writtenConfigFiles ++ mountConfigPaths);
    CREDIBLE_MOUNT_POINT = mountPoint;
    CREDIBLE_SECRET_DIR = secretDir;
    CREDIBLE_OWNER_USER = owner;
    CREDIBLE_OWNER_GROUP = group;
    CREDIBLE_PRIVATE_KEY_PATHS = commaJoin privateKeyPaths;
    CREDIBLE_EXPOSURE_CONFIGS = commaJoin exposureConfigs;
  };

  shouldAssign = val: !(val == "" || val == [ ]);

  kvequals = name: value: (optionalString (shouldAssign value) "${name}=${value}");
  makeExport = name: value: (optionalString (shouldAssign value) "export ${kvequals name value}");
  exports = concatStringsSep "\n" (mapAttrsToList makeExport environment);
  equals = concatStringsSep "\n" (mapAttrsToList kvequals environment);

  envFile = writeText "credible.env" equals;
in

{
  inherit environment exports envFile;

  systemd = {
    Unit = {
      Description = "mounting credible secrets";
    };
    Service = {
      Type = "oneshot";
      ExecStart = mountScript;

      EnvironmentFile = envFile;
    };

    Install.WantedBy = [ "network.target" ];
  };

  launchd = {
    script = ''
      set -e
      set -o pipefail
      export PATH="${gnugrep}/bin:${coreutils}/bin:@out@/sw/bin:/usr/bin:/bin:/usr/sbin:/sbin"
      ${exports}

      # Launchd does not let us delay launching wait until the
      # network is up :shrug:
      while ! route -n get 0.0.0.0 > /dev/null; do
        sleep 1
      done

      ${mountScript}
      exit 0
    '';
    serviceConfig = {
      RunAtLoad = true;
      KeepAlive.SuccessfulExit = false;
    };
  };
}
