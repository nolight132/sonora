# sonora

A minimal native Spotify client built with Rust and GPUI.

## Install

### macOS

```sh
brew install --cask nolight132/tap/sonora
```

Sonora is signed ad-hoc rather than notarized, so Gatekeeper blocks it on first
launch. Clear the quarantine attribute once, after installing:

```sh
xattr -dr com.apple.quarantine /Applications/Sonora.app
```

### Linux and Windows

Download the executable for your platform from the
[latest release](https://github.com/nolight132/sonora/releases/latest). On Linux,
mark it executable first:

```sh
chmod +x sonora-*-x86_64-unknown-linux-gnu
```

Linux needs a Vulkan driver at runtime, plus `alsa-lib`, `fontconfig`, `freetype`,
`libxkbcommon` and the X11/Wayland client libraries. `SHA256SUMS` in the release
verifies every download.

## Arch Linux / CachyOS

Install the build and runtime dependencies:

```sh
sudo pacman -S --needed base-devel rust pkgconf alsa-lib fontconfig freetype2 \
  libx11 libxcb libxcursor libxi libxkbcommon libxkbcommon-x11 wayland \
  vulkan-icd-loader
```

You also need a Vulkan driver for your GPU, such as `vulkan-radeon`,
`vulkan-intel`, or the Vulkan support included with the NVIDIA driver.

Build and run:

```sh
cargo run --locked --package sonora
```

For an optimized build:

```sh
cargo build --release --locked --package sonora
./target/release/sonora
```

The first build downloads and compiles GPUI and the other Rust dependencies,
so it can take a few minutes.

## Nix

Build from source:

```sh
nix run
```

Or run the prebuilt release binary, which skips compiling GPUI entirely:

```sh
nix run github:nolight132/sonora#sonora-bin
```

`sonora-bin` tracks the latest tagged release rather than the working tree, so its version is pinned
in `flake.nix` and only moves when a release is cut.

## License

Copyright (C) 2026 nolight132.

Sonora is free software, released under the [GNU General Public License version
3 or later](LICENSE). GPUI depends on `zlog` and `ztracing` from the Zed
repository, both GPL-3.0-or-later, so any binary built from this tree is covered
by the GPL regardless; the project follows suit. The complete corresponding
source is this repository at the matching tag, plus the revisions pinned in
`Cargo.lock`.

Sonora is an unofficial client and is not affiliated with, endorsed by, or
sponsored by Spotify AB.

The binary also embeds the [Inter](https://github.com/rsms/inter) typeface (SIL
Open Font License 1.1) and the [Lucide](https://lucide.dev) icon set (ISC
License). `THIRD-PARTY.md` lists every bundled dependency, its license and the
full license texts; regenerate it with:

```sh
cargo about generate about.hbs > THIRD-PARTY.md
```
