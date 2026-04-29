{
  description = "Rust bindings and integration scaffolding for Ada libhwbase/libgfxinit";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs { inherit system; }));
    in
    {
      packages = forAllSystems (
        pkgs:
        let
          llvm = pkgs.llvmPackages_21;
          check-llvm-ada = pkgs.writeShellApplication {
            name = "check-llvm-ada";
            runtimeInputs = [
              pkgs.coreutils
              pkgs.gnugrep
            ];
            text = ''
              set -eu
              ada_cc="''${ADA_CC:-llvm-gcc}"
              if ! command -v "$ada_cc" >/dev/null 2>&1; then
                echo "error: GNAT LLVM compiler '$ada_cc' not found in PATH" >&2
                echo "hint: build/install AdaCore gnat-llvm, then put llvm-interface/bin first in PATH" >&2
                exit 1
              fi
              version="$($ada_cc --version 2>&1 || true)"
              printf '%s\n' "$version" | grep -qi llvm || {
                echo "error: '$ada_cc' does not look like GNAT LLVM" >&2
                printf '%s\n' "$version" >&2
                exit 1
              }
              echo "using GNAT LLVM Ada compiler: $(command -v "$ada_cc")"
            '';
          };
          build-gnat-llvm = pkgs.writeShellApplication {
            name = "build-gnat-llvm";
            runtimeInputs = [
              pkgs.coreutils
              pkgs.git
              pkgs.gnumake
              pkgs.which
              pkgs.cmake
              pkgs.perl
              pkgs.python3
              pkgs.gnat15
              pkgs.gnat15Packages.gprbuild
              llvm.llvm
              llvm.clang
            ];
            text = ''
              set -euo pipefail
              dest="''${1:-$PWD/.gnat-llvm}"
              jobs="''${NIX_BUILD_CORES:-$(nproc)}"

              mkdir -p "$dest"
              cd "$dest"

              if [ ! -d gnat-llvm ]; then
                git clone https://github.com/AdaCore/gnat-llvm.git
              fi
              cd gnat-llvm

              if [ ! -d llvm-interface/gcc ]; then
                git clone --depth 1 https://gcc.gnu.org/git/gcc.git llvm-interface/gcc
              fi
              if [ ! -e llvm-interface/gnat_src ]; then
                ln -s gcc/gcc/ada llvm-interface/gnat_src
              fi
              if [ ! -d llvm-bindings ]; then
                git clone https://github.com/AdaCore/llvm-bindings.git
              fi

              export LLVM_CONFIG="${llvm.llvm}/bin/llvm-config"
              export PATH="${llvm.clang}/bin:${llvm.llvm}/bin:$PATH"
              make -j"$jobs" LLVM_CONFIG="$LLVM_CONFIG"

              printf '%s\n' \
                "GNAT LLVM built under:" \
                "  $dest/gnat-llvm/llvm-interface" \
                "" \
                "Use it with:" \
                "  export PATH=\"$dest/gnat-llvm/llvm-interface/bin:\$PATH\"" \
                "  export ADA_CC=llvm-gcc" \
                "  nix run .#check-llvm-ada"
            '';
          };
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "libgfxinit-rs";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            doCheck = true;
          };

          inherit check-llvm-ada build-gnat-llvm;
        }
      );

      apps = forAllSystems (pkgs: {
        check-llvm-ada = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.check-llvm-ada}/bin/check-llvm-ada";
        };
        build-gnat-llvm = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.build-gnat-llvm}/bin/build-gnat-llvm";
        };
      });

      checks = forAllSystems (pkgs: {
        rust = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      });

      devShells = forAllSystems (
        pkgs:
        let
          llvm = pkgs.llvmPackages_21;
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.rustc
              pkgs.rustfmt
              pkgs.clippy
              pkgs.gnumake
              pkgs.gnused
              pkgs.gawk
              pkgs.pkg-config
              pkgs.binutils
              pkgs.git
              pkgs.which
              pkgs.cmake
              pkgs.perl
              pkgs.python3
              pkgs.alire

              # Bootstrap dependencies for AdaCore/gnat-llvm.  These are not
              # the compiler used for libgfxinit artifacts; build.rs and the
              # check-llvm-ada app require ADA_CC=llvm-gcc for Ada builds.
              pkgs.gnat15
              pkgs.gnat15Packages.gprbuild

              llvm.llvm
              llvm.clang
              llvm.lld
            ];

            ADA_CC = "llvm-gcc";
            CC = "clang";
            LLVM_CONFIG = "${llvm.llvm}/bin/llvm-config";

            shellHook = ''
              echo "libgfxinit-rs development shell"
              echo "  Rust:        $(rustc --version)"
              echo "  LLVM:        $(${llvm.llvm}/bin/llvm-config --version)"
              echo "  ADA_CC:      ''${ADA_CC} (must resolve to GNAT LLVM for Ada builds)"
              if command -v "''${ADA_CC}" >/dev/null 2>&1; then
                nix run .#check-llvm-ada || true
              else
                echo "  GNAT LLVM:   not found"
                echo "  Build it with: nix run .#build-gnat-llvm -- .gnat-llvm"
                echo "  Then: export PATH=\$PWD/.gnat-llvm/gnat-llvm/llvm-interface/bin:\$PATH"
              fi
            '';
          };
        }
      );
    };
}
