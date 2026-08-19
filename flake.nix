{
	description = "SoundCloud terminal client";

	inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

	outputs = { self, nixpkgs, ... }:
		let
			systems = [
				"aarch64-darwin"
				"aarch64-linux"
				"x86_64-linux"
			];
			forAllSystems = nixpkgs.lib.genAttrs systems;
		in {
			packages = forAllSystems (system:
				let
					pkgs = nixpkgs.legacyPackages.${system};
				in {
					default = pkgs.rustPlatform.buildRustPackage {
						pname = "sctui";
						version = "0.1.0";
						src = pkgs.lib.fileset.toSource {
							root = ./.;
							fileset = pkgs.lib.fileset.unions [
								./Cargo.toml
								./Cargo.lock
								./src
							];
						};

						cargoLock.lockFile = ./Cargo.lock;
						strictDeps = true;

						nativeBuildInputs = [ pkgs.pkg-config ];
						buildInputs = [ pkgs.openssl ]
							++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.alsa-lib ];

						postInstall = ''
							test -x "$out/bin/sctui"
						'';

						meta = {
							description = "SoundCloud client for the terminal";
							homepage = "https://github.com/Illogicalll/sctui";
							license = pkgs.lib.licenses.mit;
							mainProgram = "sctui";
						};
					};
				});

			checks = forAllSystems (system: {
				default = self.packages.${system}.default;
			});

			devShells = forAllSystems (system:
				let
					pkgs = nixpkgs.legacyPackages.${system};
				in {
					default = pkgs.mkShell {
						inputsFrom = [ self.packages.${system}.default ];
						packages = [ pkgs.rustfmt ];
					};
				});
		};
}
