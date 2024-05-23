{
  description = "Tool to freeze the screen of a Wayland compositor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = {nixpkgs, ...}: let
    forAllSystems = function:
      nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
      ] (system: function nixpkgs.legacyPackages.${system});
  in {
    devShells = forAllSystems (pkgs: {
      default = pkgs.mkShell {
        buildInputs = with pkgs; [rustfmt cargo libxkbcommon];
      };
    });
    packages = forAllSystems (pkgs: rec {
      default = wayfreeze;
      wayfreeze = pkgs.rustPlatform.buildRustPackage {
        name = "wayfreeze";
        pname = "wayfreeze";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
        doCheck = true;
        nativeBuildInputs = [];
        buildInputs = with pkgs; [libxkbcommon];
      };
    });
  };
}
