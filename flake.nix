{
  description = "sonora - a minimal native Spotify client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forEachSystem = fn: nixpkgs.lib.genAttrs systems (system: fn nixpkgs.legacyPackages.${system});

      release = {
        version = "0.1.1";
        assets = {
          x86_64-linux = {
            target = "x86_64-unknown-linux-gnu";
            hash = "sha256-dMlnGAzde/A37FZKgdjB+WA5KI7/hopwfaPIacmiRnQ=";
          };
          aarch64-linux = {
            target = "aarch64-unknown-linux-gnu";
            hash = "sha256-DC1NXRpGdCnZknw22puiEhce85uCq7tDlYTe9DYShQQ=";
          };
        };
      };
    in
    {
      packages = forEachSystem (
        pkgs:
        let
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
          ];

          sonora = pkgs.rustPlatform.buildRustPackage {
            pname = "sonora";
            version = (pkgs.lib.importTOML ./Cargo.toml).workspace.package.version;

            src = ./.;

            cargoHash = "sha256-pLduNOaYtm36fpu1to7xgvVbnCqneLpY09zGeifQjSo=";

            nativeBuildInputs = with pkgs; [
              pkg-config
              mold
              bintools
            ];

            buildInputs = runtimeLibraries;

            postInstall = ''
              install -Dm444 assets/linux/sonora.desktop \
                -t "$out/share/applications"
              install -Dm444 assets/linux/sonora.svg \
                "$out/share/icons/hicolor/scalable/apps/sonora.svg"
              for icon in assets/linux/icons/hicolor/*/apps/sonora.png; do
                size="$(basename "$(dirname "$(dirname "$icon")")")"
                install -Dm444 "$icon" \
                  "$out/share/icons/hicolor/$size/apps/sonora.png"
              done
              install -Dm444 LICENSE "$out/share/licenses/sonora/LICENSE"
              install -Dm444 THIRD-PARTY.md "$out/share/licenses/sonora/THIRD-PARTY.md"
              install -Dm444 assets/fonts/LICENSE.txt \
                "$out/share/licenses/sonora/LICENSE.Inter"
              install -Dm444 assets/icons/LICENSE \
                "$out/share/licenses/sonora/LICENSE.Lucide"
            '';

            postFixup = ''
              patchelf \
                --add-rpath "${pkgs.lib.makeLibraryPath runtimeLibraries}" \
                "$out/bin/sonora"
            '';

            meta = {
              description = "A minimal native Spotify client built with GPUI";
              mainProgram = "sonora";
              license = with pkgs.lib.licenses; [
                gpl3Plus
                ofl
                isc
              ];
              platforms = pkgs.lib.platforms.linux;
            };
          };

          asset = release.assets.${pkgs.stdenv.hostPlatform.system};

          sonora-bin = pkgs.stdenv.mkDerivation {
            pname = "sonora-bin";
            inherit (release) version;

            src = pkgs.fetchurl {
              url = "https://github.com/nolight132/sonora/releases/download/v${release.version}/sonora-v${release.version}-${asset.target}";
              inherit (asset) hash;
            };

            dontUnpack = true;
            dontPatchELF = true;
            dontStrip = true;

            nativeBuildInputs = [ pkgs.makeWrapper ];

            installPhase = ''
              runHook preInstall
              install -Dm755 "$src" "$out/libexec/sonora"
              makeWrapper ${pkgs.stdenv.cc.bintools.dynamicLinker} "$out/bin/sonora" \
                --add-flags "--library-path ${
                  pkgs.lib.makeLibraryPath (runtimeLibraries ++ [ pkgs.stdenv.cc.cc.lib ])
                }" \
                --add-flags "$out/libexec/sonora"
              install -Dm444 ${./assets/linux/sonora.desktop} \
                "$out/share/applications/sonora.desktop"
              install -Dm444 ${./assets/linux/sonora.svg} \
                "$out/share/icons/hicolor/scalable/apps/sonora.svg"
              for icon in ${./assets/linux/icons}/hicolor/*/apps/sonora.png; do
                size="$(basename "$(dirname "$(dirname "$icon")")")"
                install -Dm444 "$icon" \
                  "$out/share/icons/hicolor/$size/apps/sonora.png"
              done
              install -Dm444 ${./LICENSE} "$out/share/licenses/sonora/LICENSE"
              install -Dm444 ${./THIRD-PARTY.md} "$out/share/licenses/sonora/THIRD-PARTY.md"
              install -Dm444 ${./assets/fonts/LICENSE.txt} \
                "$out/share/licenses/sonora/LICENSE.Inter"
              install -Dm444 ${./assets/icons/LICENSE} \
                "$out/share/licenses/sonora/LICENSE.Lucide"
              runHook postInstall
            '';

            meta = sonora.meta // {
              description = "A minimal native Spotify client built with GPUI (prebuilt release binary)";
            };
          };
        in
        {
          inherit sonora sonora-bin;
          default = sonora;
        }
      );

      devShells = forEachSystem (
        pkgs:
        let
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
          ];
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = with pkgs; [
              mold
              pkg-config
              rustc
              rust-analyzer
              rustfmt
              sccache
            ];

            buildInputs = runtimeLibraries;

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibraries;
          };
        }
      );
    };
}
