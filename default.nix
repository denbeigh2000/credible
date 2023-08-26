{ callPackage, naersk }:

naersk.buildPackage {
  src = ./.;

  nativeBuildInputs = callPackage ./systemLibs.nix { };
}
