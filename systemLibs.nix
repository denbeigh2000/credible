{ lib, stdenvNoCC, libudev-zero, pkg-config, darwin }:

let
  inherit (lib) optionals;
  inherit (stdenvNoCC.hostPlatform) isDarwin isLinux;
in
(optionals isDarwin [ darwin.apple_sdk.frameworks.Security ]) ++
(optionals isLinux [ libudev-zero pkg-config ])
