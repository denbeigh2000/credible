# TODO:
# - nixos module
# - home-manager module
# - library for wrapping tools

{
  nixosModule = import ./nixos.nix;
  mkWrapper =
    pkgs: config:
    import ./wrapper.nix pkgs { inherit config; };
}
