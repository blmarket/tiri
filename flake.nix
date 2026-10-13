{
  description = "tiri development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";
  inputs.rust-overlay.inputs.nixpkgs.follows = "nixpkgs";

  outputs = { self, nixpkgs, rust-overlay, ... }:
    let
      supportedSystems = [
        "aarch64-darwin"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      rustStableFor = pkgs: pkgs.rust-bin.stable.latest.minimal;
      rustNightlyFor = pkgs: pkgs.rust-bin.nightly.latest.default.override {
        extensions = [ "rust-src" "rust-analyzer" "llvm-tools-preview" ];
      };
      cargoLlvmCovFor = pkgs: pkgs.callPackage ./nix/cargo-llvm-cov.nix { };
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          rustNightly = rustNightlyFor pkgs;
          # Build with the same nightly toolchain the dev shell uses; the crate
          # needs `extern_types` and `local_waker`.
          rustPlatform = pkgs.makeRustPlatform {
            cargo = rustNightly;
            rustc = rustNightly;
          };
          tiri = rustPlatform.buildRustPackage {
            pname = "tiri";
            version = "0.0.0";
            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;

            # build.rs links these unconditionally, which is also why this
            # package is Linux-only.
            buildInputs = [
              pkgs.ncurses
              pkgs.utf8proc
              pkgs.libutempter
              pkgs.systemd
            ];

            # The unit tests reach into the server's process-wide state and
            # need a process each; `make unit-c2rs` is where they run.
            doCheck = false;

            meta.mainProgram = "tiri";
          };
        in
        {
          tmux-target = pkgs.callPackage ./nix/tmux-target.nix { };
          tmux = self.packages.${system}.tmux-target;
        }
        // nixpkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.isLinux) {
          inherit tiri;
          default = tiri;
        });

      devShells = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          tmuxTarget = self.packages.${system}.tmux-target;
          llvm = pkgs.llvmPackages;
          rustStable = rustStableFor pkgs;
          rustNightly = rustNightlyFor pkgs;
          cargoLlvmCov = cargoLlvmCovFor pkgs;
        in
        {
          default = pkgs.mkShell ({
            shellHook = ''
              export SHELL="${pkgs.bashInteractive}/bin/bash"
            '';

            TMUX_UPDATE_REFERENCE = "${tmuxTarget}/bin/tmux";
            TMUX_UPDATE_TARGET = "${tmuxTarget}/bin/tmux";
            RUST_SRC_PATH = "${rustNightly}/lib/rustlib/src/rust/library";

            ASAN_SYMBOLIZER_PATH = "${llvm.llvm}/bin/llvm-symbolizer";

            CMAKE_LLVM_DIR = "${llvm.libllvm.dev}/lib/cmake/llvm";
            CMAKE_CLANG_DIR = "${llvm.libclang.dev}/lib/cmake/clang";
            LLVM_CONFIG_PATH = "${llvm.libllvm.dev}/bin/llvm-config";
            CLANG_PATH = "${llvm.clang}/bin/clang";
            LIBCLANG_PATH = "${llvm.libclang.lib}/lib";

            C2RUST_BUILD_CARGO = "${rustStable}/bin/cargo";
            C2RUST_BUILD_RUSTC = "${rustStable}/bin/rustc";

            packages = [
              pkgs.bashInteractive
              tmuxTarget
              rustNightly
              pkgs.cargo-nextest
              cargoLlvmCov
              pkgs.gnumake
              pkgs.git
              pkgs.python3
              pkgs.jq
              pkgs.bear
              pkgs.cmake
              pkgs.pkg-config
              llvm.clang
              llvm.llvm
              llvm.libclang
              llvm.libllvm
            ]
            # Valgrind does not build for aarch64-darwin.
            ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
              pkgs.valgrind
            ];

            buildInputs = [
              pkgs.openssl
              pkgs.zlib
              llvm.libclang
              llvm.libllvm
            ]
            ++ (tmuxTarget.buildInputs or [ ]);

            nativeBuildInputs = tmuxTarget.nativeBuildInputs or [ ];
          } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath
              (map (p: p.lib or p.out or p) (tmuxTarget.buildInputs or [ ]));
            LOCALE_ARCHIVE = "${pkgs.glibcLocales}/lib/locale/locale-archive";
            LOCALE_ARCHIVE_2_27 = "${pkgs.glibcLocales}/lib/locale/locale-archive";
          });
        });
    };
}
