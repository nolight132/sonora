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
            '';

            postFixup = ''
              patchelf \
                --add-rpath "${pkgs.lib.makeLibraryPath runtimeLibraries}" \
                "$out/bin/sonora"
            '';

            meta = {
              description = "A minimal native Spotify client built with GPUI";
              mainProgram = "sonora";
              platforms = pkgs.lib.platforms.linux;
            };
          };
        in
        {
          inherit sonora;
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
