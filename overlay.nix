{ naersk }:

final: prev:

let
  inherit (prev) callPackage;
  naersk' = callPackage naersk { };
in
{
  credible = callPackage ./. { naersk = naersk'; };
}
