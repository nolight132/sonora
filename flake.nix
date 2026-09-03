{
  description = "Sonora - a native music streaming client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      inherit (nixpkgs) lib;
      release = {
        version = "0.30.0";
        assets = {
          x86_64-linux = {
            target = "x86_64-unknown-linux-gnu";
            hash = "sha256-vg8RIkzOuNOB0c/I6HPx7wPNPyuF6J7unCDiMVa7ha0=";
          };
          aarch64-linux = {
            target = "aarch64-unknown-linux-gnu";
            hash = "sha256-hxh1MIgT1/j+OgJq9+2MLDLADw2XRfTWsR0T4nmoKSs=";
          };
        };
      };
    in
    {
      packages = lib.genAttrs lib.systems.flakeExposed (
        system:
        let
          pkgs = import nixpkgs { inherit system; };

          runtimeLibraries = with pkgs; [
            vulkan-loader
            wayland
            libxkbcommon
            libxcb
            libx11
            libxcursor
            libxi
            fontconfig
            freetype
            alsa-lib
            dbus
            sqlite
          ];

          postInstall = ''
            install -Dm444 ${./assets/linux/sonora.desktop} \
              "$out/share/applications/sonora.desktop"
            install -Dm444 ${./assets/linux/sonora.svg} \
              "$out/share/icons/hicolor/scalable/apps/sonora.svg"
            for icon in ${./assets/linux/icons}/hicolor/*/apps/sonora.png; do
              size="$(basename "$(dirname "$(dirname "$icon")")")"
              install -Dm444 "$icon" \
                "$out/share/icons/hicolor/$size/apps/sonora.png"
            done
            install -Dm444 ${./COPYING} "$out/share/licenses/sonora/LICENSE"
            install -Dm444 ${./THIRD-PARTY.md} "$out/share/licenses/sonora/THIRD-PARTY.md"
            install -Dm444 ${./assets/fonts/LICENSE.txt} \
              "$out/share/licenses/sonora/LICENSE.Inter"
            for licence in ${./assets/icons}/*/LICENSE; do
              pack="$(basename "$(dirname "$licence")")"
              install -Dm444 "$licence" \
                "$out/share/licenses/sonora/icons/LICENSE.$pack"
            done
            install -Dm444 ${./assets/icons/LICENSE} \
              "$out/share/licenses/sonora/icons/LICENSE"
          '';

          meta = {
            description = "A native music streaming client, built with Rust and GPUI";
            mainProgram = "sonora";
            license = with lib.licenses; [
              gpl3Plus
              ofl
              isc
            ];
            platforms = lib.platforms.linux;
          };

          sonora = pkgs.rustPlatform.buildRustPackage (final: {
            name = "sonora";
            version = (lib.importTOML (final.src + /Cargo.toml)).workspace.package.version;

            src = ./.;
            cargoLock = {
              lockFile = final.src + /Cargo.lock;
              allowBuiltinFetchGit = true;
            };

            nativeBuildInputs =
              with pkgs;
              lib.flatten [
                cmake
                pkg-config
                (lib.optionals stdenv.hostPlatform.isLinux [
                  autoPatchelfHook
                  mold
                ])
              ];
            buildInputs =
              with pkgs;
              lib.flatten [
                sqlite
                (lib.optionals stdenv.hostPlatform.isLinux [
                  dbus
                  fontconfig
                  libxcb
                  libxkbcommon
                  libX11
                  pipewire
                  stdenv.cc.cc.lib
                  (alsa-lib-with-plugins.override {
                    plugins = [
                      alsa-plugins
                      pipewire
                    ];
                  })
                ])
                (lib.optionals stdenv.hostPlatform.isDarwin [
                  apple-sdk_15
                  (darwinMinVersionHook "10.15")
                ])
              ];
            runtimeDependencies =
              with pkgs;
              lib.optionals stdenv.hostPlatform.isLinux [
                vulkan-loader
                wayland
              ];

            inherit postInstall;

            meta = meta // {
              platforms = lib.platforms.all;
            };
          });

          asset = release.assets.${pkgs.stdenv.hostPlatform.system};

          sonora-bin = pkgs.stdenv.mkDerivation {
            pname = "sonora-bin";
            inherit (release) version;

            src = pkgs.fetchurl {
              url = "https://github.com/nolight132/sonora/releases/download/v${release.version}/sonora-v${release.version}-${asset.target}";
              inherit (asset) hash;
            };

            dontUnpack = true;
            dontStrip = true;

            installPhase = ''
              runHook preInstall
              install -Dm755 "$src" "$out/bin/sonora"
              runHook postInstall
            '';

            postFixup = ''
              patchelf \
                --set-interpreter "${pkgs.stdenv.cc.bintools.dynamicLinker}" \
                --add-rpath "${lib.makeLibraryPath (runtimeLibraries ++ [ pkgs.stdenv.cc.cc.lib ])}" \
                "$out/bin/sonora"
            '';

            inherit postInstall meta;
          };
        in
        {
          inherit sonora;
          default = sonora;
        }
        // lib.optionalAttrs (builtins.hasAttr system release.assets) {
          inherit sonora-bin;
          default = sonora-bin;
        }
      );

      devShells = lib.genAttrs lib.systems.flakeExposed (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs =
              with pkgs;
              [
                pkg-config
                cmake
                rustToolchain
                sccache
              ]
              ++ lib.optionals pkgs.stdenv.hostPlatform.isLinux [
                mold
              ];

            buildInputs = self.packages.${system}.sonora.buildInputs;

            LD_LIBRARY_PATH = lib.makeLibraryPath self.packages.${system}.sonora.runtimeDependencies;

            shellHook = lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              if [ ! -d /run/opengl-driver ]; then
                export VK_DRIVER_FILES="${pkgs.mesa}/share/vulkan/icd.d"
                export VK_IMPLICIT_LAYER_PATH="${pkgs.mesa}/share/vulkan/implicit_layer.d"
              fi
            '';
          };
        }
      );
    };
}
