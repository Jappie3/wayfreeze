{
  description = "Tool to freeze the screen of a wlroots compositor";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs = {
    self,
    nixpkgs,
    flake-parts,
    ...
  } @ inputs:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux"];
      perSystem = {
        pkgs,
        system,
        self',
        ...
      }: {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [rustfmt cargo libxkbcommon];
        };
        packages = {
          default = self'.packages.wayfreeze;
          wayfreeze = pkgs.rustPlatform.buildRustPackage {
            name = "Wayfreeze";
            pname = "Wayfreeze";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            doCheck = true;
            nativeBuildInputs = with pkgs; [rustfmt rustc cargo];
            buildInputs = with pkgs; [];
          };
        };
      };
    };
}
