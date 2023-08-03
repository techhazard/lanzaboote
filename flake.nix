{
  nixConfig.extra-substituters = [
    "https://nix-community.cachix.org"
  ];
  nixConfig.extra-trusted-public-keys = [
    "nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs="
  ];

  description = "Lanzaboote: Secure Boot for NixOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable-small";

    flake-parts.url = "github:hercules-ci/flake-parts";
    flake-parts.inputs.nixpkgs-lib.follows = "nixpkgs";

    pre-commit-hooks-nix = {
      url = "github:cachix/pre-commit-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
      inputs.flake-compat.follows = "flake-compat";
    };

    # We only have this input to pass it to other dependencies and
    # avoid having multiple versions in our dependencies.
    flake-utils.url = "github:numtide/flake-utils";

    flake-compat = {
      url = "github:edolstra/flake-compat";
      flake = false;
    };
  };

  outputs = inputs@{ self, nixpkgs, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } ({ moduleWithSystem, ... }: {
      imports = [
        # Derive the output overlay automatically from all packages that we define.
        inputs.flake-parts.flakeModules.easyOverlay

        # Formatting and quality checks.
        inputs.pre-commit-hooks-nix.flakeModule
      ];

      flake.nixosModules.lanzaboote = moduleWithSystem (perSystem@{ config }:
        { ... }: {
          imports = [ ./nix/modules/lanzaboote.nix ];

          boot.lanzaboote.package = perSystem.config.packages.tool;
        });

      flake.nixosModules.uki = moduleWithSystem (perSystem@{ config }:
        { lib, ... }: {
          imports = [ ./nix/modules/uki.nix ];

          boot.loader.uki.stub = lib.mkDefault
            "${perSystem.config.packages.fatStub}/bin/lanzaboote_stub.efi";
        });

      systems = [
        "x86_64-linux"

        # Not actively tested, but may work:
        "aarch64-linux"
      ];

      perSystem = { config, system, pkgs, ... }:
        let
          pkgs = import nixpkgs { inherit system; };
          uefiPkgs = import nixpkgs {
            inherit system;
            crossSystem = {
              # linuxArch is wrong here, it will yield arm64 instead of aarch64.
              config = "${pkgs.stdenv.hostPlatform.qemuArch}-windows";
              rustc.config = "${pkgs.stdenv.hostPlatform.qemuArch}-unknown-uefi";
              libc = null;
              useLLVM = true;
            };
          };
          utils = import ./nix/packages/utils.nix;

          inherit (pkgs) lib;

          stub = uefiPkgs.callPackage ./nix/packages/stub.nix { };
          fatStub =
            uefiPkgs.callPackage ./nix/packages/stub.nix { fatVariant = true; };
          tool = pkgs.callPackage ./nix/packages/tool.nix { };

          wrappedTool = pkgs.runCommand "lzbt"
            {
              nativeBuildInputs = [ pkgs.makeWrapper ];
            } ''
            mkdir -p $out/bin

            # Clean PATH to only contain what we need to do objcopy. Also
            # tell lanzatool where to find our UEFI binaries.
            makeWrapper ${tool}/bin/lzbt $out/bin/lzbt \
              --set PATH ${
                lib.makeBinPath [ pkgs.binutils-unwrapped pkgs.sbsigntool ]
              } \
              --set LANZABOOTE_STUB ${stub}/bin/lanzaboote_stub.efi
          '';
        in
        {
          packages = {
            inherit stub fatStub;
            tool = wrappedTool;
            lzbt = wrappedTool;
          };

          overlayAttrs = { inherit (config.packages) tool; };

          checks =
            let
              nixosLib = import (pkgs.path + "/nixos/lib") { };
              runTest = module:
                nixosLib.runTest {
                  imports = [ module ];
                  hostPkgs = pkgs;
                };
            in
            {
              stubFmt = uefiPkgs.callPackage (utils.rustfmt stub) { };
              toolFmt = pkgs.callPackage (utils.rustfmt tool) { };
              toolClippy = pkgs.callPackage (utils.clippy tool) { };
              stubClippy = uefiPkgs.callPackage (utils.clippy stub) { };
              fatStubClippy = uefiPkgs.callPackage (utils.clippy fatStub) { };
            } // (import ./nix/tests/lanzaboote.nix {
              inherit pkgs;
              lanzabooteModule = self.nixosModules.lanzaboote;
            }) // (import ./nix/tests/stub.nix {
              inherit pkgs runTest;
              ukiModule = self.nixosModules.uki;
            });

          pre-commit = {
            check.enable = true;

            settings.hooks = {
              nixpkgs-fmt.enable = true;
              typos.enable = true;
            };
          };

          devShells.default = pkgs.mkShell {
            shellHook =
              let
                systemdUkify = pkgs.systemdMinimal.override {
                  withEfi = true;
                  withUkify = true;
                };
              in
              ''
                ${config.pre-commit.installationScript}
                export PATH=$PATH:${systemdUkify}/lib/systemd
              '';

            packages = [
              pkgs.uefi-run
              pkgs.openssl
              (pkgs.sbctl.override { databasePath = "pki"; })
              pkgs.sbsigntool
              pkgs.efitools
              pkgs.python39Packages.ovmfvartool
              pkgs.qemu
              pkgs.nixpkgs-fmt
              pkgs.statix
              pkgs.cargo-release
            ];

            inputsFrom = [ config.packages.stub config.packages.tool ];

            TEST_SYSTEMD = pkgs.systemd;
          };
        };
    });
}
