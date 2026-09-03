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
      nixpkgs,
      rust-overlay,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      forEachSystem =
        fn:
        nixpkgs.lib.genAttrs systems (
          system:
          let
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ (import rust-overlay) ];
            };
          in
          fn pkgs
        );

      release = {
        version = "0.29.0";
        assets = {
          x86_64-linux = {
            target = "x86_64-unknown-linux-gnu";
            hash = "sha256-FUKrUrfBKKOMMzaqAJHxKMjHK/nBcjLa3qqe8IZPWA4=";
          };
          aarch64-linux = {
            target = "aarch64-unknown-linux-gnu";
            hash = "sha256-XRfVINLgVp4pcGpYqHilBN9F2+z5b3afU5iJXh7dAtw=";
          };
          aarch64-darwin = {
            target = "macos";
            hash = "sha256-MG2bn4iN47W7HmWWoHlLDS1s7kN9PL4hYC1MnDSM3jk=";
          };
        };
      };
    in
    {
      packages = forEachSystem (
        pkgs:
        let
          runtimeLibraries =
            with pkgs;
            if pkgs.stdenv.hostPlatform.isLinux then
              [
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
              ]
            else
              [ ];

          asset = release.assets.${pkgs.stdenv.hostPlatform.system};

          sonora-bin = pkgs.stdenv.mkDerivation {
            pname = "sonora-bin";
            inherit (release) version;

            src = pkgs.fetchurl {
              url = "https://github.com/nolight132/sonora/releases/download/v${release.version}/sonora-v${release.version}-${asset.target}${if pkgs.stdenv.hostPlatform.isDarwin then ".dmg" else ""}";
              inherit (asset) hash;
            };

            nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
              pkgs.makeBinaryWrapper
            ];

            dontUnpack = true;
            dontStrip = true;

            installPhase =
              if pkgs.stdenv.hostPlatform.isLinux then
                ''
                  runHook preInstall
                  install -Dm755 "$src" "$out/bin/sonora"
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
                  runHook postInstall
                ''
              else
                ''
                  runHook preInstall
                  mnt="$(mktemp -d)"
                  /usr/bin/hdiutil attach -readonly -nobrowse -mountpoint "$mnt" "$src"
                  mkdir -p "$out/Applications" "$out/bin"
                  cp -R "$mnt/Sonora.app" "$out/Applications/Sonora.app"
                  /usr/bin/hdiutil detach "$mnt"
                  makeBinaryWrapper \
                    "$out/Applications/Sonora.app/Contents/MacOS/sonora" \
                    "$out/bin/sonora"
                  runHook postInstall
                '';

            postFixup = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
              patchelf \
                --set-interpreter "${pkgs.stdenv.cc.bintools.dynamicLinker}" \
                --add-rpath "${pkgs.lib.makeLibraryPath (runtimeLibraries ++ [ pkgs.stdenv.cc.cc.lib ])}" \
                "$out/bin/sonora"
            '';

            meta = {
              description = "A native music streaming client, built with Rust and GPUI";
              mainProgram = "sonora";
              license = with pkgs.lib.licenses; [
                gpl3Plus
                ofl
                isc
              ];
              platforms =
                if pkgs.stdenv.hostPlatform.isLinux then pkgs.lib.platforms.linux else pkgs.lib.platforms.darwin;
            };
          };
        in
        {
          inherit sonora-bin;
          sonora = sonora-bin;
          default = sonora-bin;
        }
      );

      devShells = forEachSystem (
        pkgs:
        let
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          runtimeLibraries =
            with pkgs;
            if pkgs.stdenv.hostPlatform.isLinux then
              [
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
              ]
            else
              [ ];
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs =
              (with pkgs; [
                pkg-config
                cmake
                rustToolchain
                sccache
              ])
              ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.mold ];

            buildInputs = runtimeLibraries;

            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibraries;

            ALSA_PLUGIN_DIR =
              if pkgs.stdenv.hostPlatform.isLinux then
                "${pkgs.symlinkJoin {
                  name = "alsa-plugins-combined";
                  paths = [
                    "${pkgs.alsa-plugins}/lib/alsa-lib"
                    "${pkgs.pipewire}/lib/alsa-lib"
                  ];
                }}"
              else
                "";

            shellHook = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
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
