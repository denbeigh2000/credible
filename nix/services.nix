{ lib
, credible
, writeText
, configFile
, secretDir
, secretRoot
, owner
, group
, privateKeyPaths
}:

let
  mountScript = ''
    if [[ -e "${secretDir}" ]]
    then
      ${credible}/bin/credible unmount
    fi

    ${credible}/bin/credible mount
  '';

  envrionment = {
    CREDIBLE_CONFIG_FILE = configFile;
    CREDIBLE_MOUNT_POINT = secretRoot;
    CREDIBLE_SECRET_DIR = secretDir;
    CREDIBLE_OWNER_USER = user;
    CREDIBLE_OWNER_GROUP = group;
    CREDIBLE_PRIVATE_KEY_PATHS = lib.concatStringSep "," privateKeyPaths;
  };

  kvequals = name: value: "${name}=${value}";
  makeExport = name: value: "export ${equals name value}";
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
      export PATH="${pkgs.gnugrep}/bin:${pkgs.coreutils}/bin:@out@/sw/bin:/usr/bin:/bin:/usr/sbin:/sbin"
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
