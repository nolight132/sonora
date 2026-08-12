<div align="center">

# Sonora

[![Build](https://img.shields.io/github/actions/workflow/status/nolight132/sonora/release.yml)](https://github.com/nolight132/sonora/actions/workflows/release.yml)
[![License](https://img.shields.io/github/license/nolight132/sonora)](./COPYING)

### A native streaming client built with Rust and GPUI.
This project would not be possible without [librespot](https://github.com/librespot-org/librespot).
</div>

<div align="center">
    <table>
      <tr>
        <td colspan="2">
          <img width="1499" height="935" alt="image" src="https://github.com/user-attachments/assets/30202af9-5612-4107-8ef5-679b2b9c16e8" />
        </td>
      </tr>
      <tr>
        <td width="50%">
          <img width="1499" height="935" alt="image" src="https://github.com/user-attachments/assets/7f6cddb1-90ab-4f0a-a7f4-a7ba0081119f" />
        </td>
        <td width="50%">
          <img width="1499" height="935" alt="image" src="https://github.com/user-attachments/assets/76a73338-8cbf-4cb6-8c05-decc6a481671" />
        </td>
      </tr>
    </table>
</div>
<div align="center">
    <sub>
      Adaptive themes are optional. Everything is (or will be) customizable.
    </sub>
</div>

## Features
- **Spotify**, **YouTube**, and local playback.
- Library management within supported providers.
- Gapless playback (Spotify).
- Synced lyrics.
- Cross-platform support.
- Audio normalization.
- Custom themes.
- Offline local playback.

## Install

### macOS

```sh
brew install --cask nolight132/tap/sonora
```
After installing (thanks Apple):

```sh
xattr -dr com.apple.quarantine /Applications/Sonora.app
```

### Linux

Arch and derivatives, from the AUR:

```sh
paru -S sonora-bin        # the released binary
paru -S sonora            # built from source
```

COPR and .deb coming soon.

### Nix
Just use the flake in the project root.

```sh
inputs.sonora.packages.${system}.sonora-bin
```

`sonora-bin` tracks the latest tagged release.

### Windows
Download the latest `.exe` for your architecture from [Releases](https://github.com/nolight132/sonora/releases/latest). There isn't a proper Windows installer yet.

## Translations

Sonora ships these locales. Anything a locale is missing falls back to English at runtime, so a
partial translation is welcome — pick a language below and fill in what it lacks. Strings live in
`assets/i18n/<locale>/main.ftl`; `en-US` is the source of truth.

<!-- i18n:start -->

| Language | Translated | Coverage |
| --- | --- | --- |
| English (`en-US`) | 328/328 | 100% |
| Русский (`ru`) | 328/328 | 100% |
| Українська (`uk`) | 328/328 | 100% |
| Polski (`pl`) | 328/328 | 100% |

<!-- i18n:end -->

To add a language, create `assets/i18n/<locale>/main.ftl` and register it in
`crates/i18n/src/language.rs`. Regenerate the table with `scripts/i18n-coverage.py`.

## License

Copyright (C) 2026 Sonora Contributors.

Sonora is free software, released under the [GNU General Public License version
3 or later](COPYING).

Sonora is an unofficial client and is not affiliated with, endorsed by, or
sponsored by Spotify AB.

The binary also embeds the [Inter](https://github.com/rsms/inter) typeface (SIL
Open Font License 1.1) and the [Lucide](https://lucide.dev) icon set (ISC
License). `THIRD-PARTY.md` lists every bundled dependency.
