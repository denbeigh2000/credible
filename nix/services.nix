{ lib
, credible
, writeText
, configFile
, secretDir
, mountPoint
, owner
, group
, privateKeyPaths
, gnugrep
, coreutils
}:

let
  inherit (lib) mapAttrsToList concatStringsSep;

  mountScript = ''
    if [[ -e "${secretDir}" ]]
    then
      ${credible}/bin/credible unmount
    fi

    ${credible}/bin/credible mount
  '';

  writtenConfigFile = writeText "credible.json" (builtins.toJSON configFile);

  environment = {
    CREDIBLE_CONFIG_FILE = writtenConfigFile;
    CREDIBLE_MOUNT_POINT = mountPoint;
    CREDIBLE_SECRET_DIR = secretDir;
    CREDIBLE_OWNER_USER = owner;
    CREDIBLE_OWNER_GROUP = group;
    CREDIBLE_PRIVATE_KEY_PATHS = concatStringsSep "," privateKeyPaths;
  };

  kvequals = name: value: "${name}=${value}";
  makeExport = name: value: "export ${kvequals name value}";
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

    Install.WantedBy = ["network.target"];
  };

  launchd = {
    script = ''
      set -e
      set -o pipefail
      export PATH="${gnugrep}/bin:${coreutils}/bin:@out@/sw/bin:/usr/bin:/bin:/usr/sbin:/sbin"
      ${exports}

      ${mountScript}
      exit 0
    '';
    serviceConfig = {
      RunAtLoad = true;
      KeepAlive.SuccessfulExit = false;
    };
  };
}
