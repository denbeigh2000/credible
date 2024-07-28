{ lib }:

let
  inherit (lib) mapAttrsToList concatStringsSep optionalString;
  inherit (builtins) map;

  isEmpty = val: (val == "" || val == [ ]);
  kvequals = name: value: (optionalString (!isEmpty value) "${name}=${value}");
  makeExport = name: value: (optionalString (!isEmpty value) "export ${kvequals name value}");
in

{

  # sample file contents:
  # export CREDIBLE_X=y
  # export CREDIBLE_Y=z
  mkExports = environment:
    concatStringsSep "\n" (mapAttrsToList makeExport environment);

  # sample file contents:
  # CREDIBLE_X=y
  # CREDIBLE_Y=z
  mkEnv = environment: concatStringsSep "\n" (mapAttrsToList kvequals environment);
}
