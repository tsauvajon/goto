{
  description = "goto — URL shortener with an API, a CLI, and a Frontend";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "goto";
          version = "2.0.2";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;
          # The integration tests touch the network and a real bookmarks
          # database under $HOME, neither of which works in a Nix
          # sandbox build.
          doCheck = false;
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/goto";
        };

        devShells.default = pkgs.mkShell { packages = [ pkgs.cargo pkgs.rustc ]; };

        formatter = pkgs.nixfmt-rfc-style;
      }
    )
    // {
      # Home Manager module exposing `programs.goto`.
      homeManagerModules.default = import ./hm-module.nix self;
    };
}
