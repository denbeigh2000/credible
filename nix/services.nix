{ lib
, callPackage
, credible
, writeText
, configFiles
, secretDir
, mountPoint
, owner
, group
, privateKeyPaths
, gnugrep
, coreutils
}:

let
  inherit (lib) mapAttrsToList concatStringsSep optionalString;
  inherit (builtins) map;

  libshell = callPackage ./libshell.nix { };

  writeFile = f: writeText "credible.json" (builtins.toJSON f);

  mountScript = ''
    ${credible}/bin/credible system mount
  '';

  writtenConfigFiles = map writeFile configFiles;

  commaJoin = things: concatStringsSep "," things;

  environment = {
    CREDIBLE_CONFIG_FILES = commaJoin writtenConfigFiles;
    CREDIBLE_MOUNT_POINT = mountPoint;
    CREDIBLE_SECRET_DIR = secretDir;
    CREDIBLE_OWNER_USER = owner;
    CREDIBLE_OWNER_GROUP = group;
    CREDIBLE_PRIVATE_KEY_PATHS = commaJoin privateKeyPaths;
  };

  exports = libshell.makeExports environment;
  equals = libshell.makeEnv environment;

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

    Install = {
      Wants = [ "network.target" ];
      After = [ "network.target" ];
    };
  };

  launchd = {
    # NOTE: there doesn't seem to be a reasonable way to have tasks depend on
    # this finishing in MacOS
    # https://apple.stackexchange.com/a/402925
    # https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html
    # > The `launchd` daemon was designed to remove the need for dependency ordering among daemons
    script = ''
      set -e
      set -o pipefail
      # TODO: move this to a real script
      export PATH="${gnugrep}/bin:${coreutils}/bin:@out@/sw/bin:/usr/bin:/bin:/usr/sbin:/sbin"

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
