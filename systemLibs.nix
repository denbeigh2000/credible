{ stdenvNoCC, libudev-zero, pkg-config, darwin }:

let
  inherit (stdenvNoCC.hostPlatform) isDarwin isLinux;
  linuxOnlyPkgs = [ libudev-zero pkg-config ];
  darwinOnlyPkgs = [ darwin.apple_sdk.frameworks.Security ];
in
(if isDarwin then darwinOnlyPkgs else [ ]) ++
(if isLinux then linuxOnlyPkgs else [ ])
