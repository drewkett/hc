{
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";
  inputs.flake-utils.follows = "rust-overlay/flake-utils";
  inputs.nixpkgs.follows = "rust-overlay/nixpkgs";
  outputs = inputs @ { nixpkgs, flake-utils, rust-overlay, ... } :
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = nixpkgs.legacyPackages.${system}; in
      {
        packages = rec {
          hcp = pkgs.rustPlatform.buildRustPackage rec {
            name = "hcp";
            version = "0.2.0";

            src = pkgs.fetchFromGitHub {
              owner = "drewkett";
              repo = name;
              rev = version;
              hash = "sha256-IpGKAVXMg05NpeHQMlD3UKzOKBDL7dm4KjMoaGaJuhI";
            };

            cargoSha256 =
              "sha256-m+2DWLH945GR7tiyurjJVOeQD7SFrVpxLVx7TQZELyE";
          };
          default = hcp;
        };
        devShell = rec {
          default = pkgs.mkShell {
            buildInputs = [ pkgs.cargo pkgs.rustc ];
          };
        };
      });
}
