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

```sh
nix run
```
