{
  description = "Tool to freeze the screen of a Wayland compositor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
  };

  outputs =
    { nixpkgs, systems, ... }:
    let
      forAllSystems =
        function: nixpkgs.lib.genAttrs (import systems) (system: function nixpkgs.legacyPackages.${system});
    in
    {
      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustfmt
            cargo
            libxkbcommon
          ];
        };
      });
      packages = forAllSystems (pkgs: rec {
        default = wayfreeze;
        wayfreeze = pkgs.rustPlatform.buildRustPackage {
          name = "wayfreeze";
          src = ./.;
          cargoHash = "sha256-pB314ytkSOxPIv24urNfMbyq3KfYs3MOmivicq267/c=";
          doCheck = true;
          nativeBuildInputs = [ ];
          buildInputs = with pkgs; [ libxkbcommon ];
        };
      });
    };
}
