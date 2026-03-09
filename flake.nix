{
  description = "via - Issue commands across multiple interactive CLI sessions";

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
          # Tests require a real PTY which doesn't work in the Nix sandbox
          teetty = pkgs.teetty.overrideAttrs { doCheck = false; };
        in
        {
          default = pkgs.callPackage ./default.nix {
            inherit teetty;
          };
          via = self.packages.${system}.default;
        }
      );

      devShells = eachSystem (system:
        let
          pkgs = import nixpkgs {
            inherit system;
          };
          teetty = pkgs.teetty.overrideAttrs { doCheck = false; };
        in
        {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              cargo
              rustc
              rust-analyzer
              rustfmt
              clippy
              teetty
            ];
            inputsFrom = [ self.packages.${system}.default ];
          };
        }
      );
    };
}
