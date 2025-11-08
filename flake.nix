{
  description = "via - Hacky script for use with teetty";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/pull/459579/head";
    systems.url = "github:nix-systems/default";
  };

  outputs = { self, nixpkgs, systems }:
    let
      eachSystem = nixpkgs.lib.genAttrs (import systems);
    in
    {
      packages = eachSystem (system:
        let
          pkgs = import nixpkgs {
            inherit system;
          };
        in
        {
          default = pkgs.callPackage ./default.nix {
            inherit (pkgs) teetty;
          };
          via = self.packages.${system}.default;
        }
      );

      devShells = eachSystem (system:
        let
          pkgs = import nixpkgs {
            inherit system;
          };
        in
        {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              teetty
            ];
            inputsFrom = [ self.packages.${system}.default ];
          };
        }
      );
    };
}
