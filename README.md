<div align="center">

# Sonora

[![Build](https://img.shields.io/github/actions/workflow/status/nolight132/sonora/release.yml)](https://github.com/nolight132/sonora/actions/workflows/release.yml)
[![License](https://img.shields.io/github/license/nolight132/sonora)](./COPYING)

### A native music streaming client, built with Rust and GPUI.
Stream Spotify, YouTube Music, and local files all in one **native** app.
</div>

<div align="center">
    <table>
      <tr>
        <td colspan="2">
          <img width="1602" height="992" alt="image" src="https://github.com/user-attachments/assets/d0357517-a28d-4c90-abd1-4f3e8d8cdedc" />
        </td>
      </tr>
      <tr>
        <td width="50%">
          <img width="1576" height="945" alt="image" src="https://github.com/user-attachments/assets/70979e4c-261f-4561-b671-04d28a9971a9" />
        </td>
        <td width="50%">
          <img width="1576" height="945" alt="image" src="https://github.com/user-attachments/assets/ff3b4284-25e2-4487-bf9b-60d8f56dc44d" />
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
- Gapless playback.
- Audio normalization.
- Synced/karaoke lyrics.
- Romanization.
- Cross-platform support.
- Custom themes.

## Install

### macOS

Install with [Brew](https://brew.sh/).

```sh
brew install --cask nolight132/tap/sonora
```
After installing (thanks Apple):

```sh
xattr -dr com.apple.quarantine /Applications/Sonora.app
```

### Linux

#### Arch

Install from the AUR with your AUR helper of choice.

```
yay -S sonora
```

#### Other

Flatpak coming soon.

### Nix
Just use the flake in the project root.

```sh
inputs.sonora.packages.${system}.default
```

The flake installs the latest tagged release binary.

### Windows

#### Installer

Download and run the [installer](https://github.com/nolight132/sonora/releases/latest/download/Sonora-Setup.exe).

#### Portable

Download the latest `windows-msvc.exe` for your architecture from [Releases](https://github.com/nolight132/sonora/releases/latest).

## Translations

Sonora ships these locales. Anything a locale is missing falls back to English at runtime, so a
partial translation is welcome — pick a language below and fill in what it lacks. Strings live in
`assets/i18n/<locale>/main.ftl`; `en-US` is the source of truth.

<!-- i18n:start -->

| Language | Translated | Coverage |
| --- | --- | --- |
| English (`en-US`) | 417/417 | 100% |
| Deutsch (`de`) | 417/417 | 100% |
| Français (`fr`) | 417/417 | 100% |
| Русский (`ru`) | 417/417 | 100% |
| Українська (`uk`) | 417/417 | 100% |
| Polski (`pl`) | 417/417 | 100% |

<!-- i18n:end -->

To add a language, create `assets/i18n/<locale>/main.ftl` and register it in
`crates/i18n/src/language.rs`. Regenerate the table with `scripts/i18n-coverage.py`.

## AI policy
We have nothing against the usage of LLMs in the project — in fact, we use them ourselves. 
We believe that AI can speed up development in a lot of meaningful ways and be a useful 
tool for learning new concepts. We have also found it particularly helpful for 
contributing to substantial codebases such as GPUI and librespot, where it has helped 
us quickly locate the relevant parts of the code.

**However**, using AI cannot act as an excuse for failing to 
understand, review, and test the changes proposed. Furthermore, we expect communication 
with a real person, not a computer. This includes but is not limited to PR/issue text
generation, comments in discussions, etc. A summary of changes can be generated
and does not need to be disclosed explicitly, but the reasoning and motivation
behind a change must come from the contributor and reflect their own understanding.

AI-assisted proofreading and translation of human-written text are permitted.

## Credits

Sonora is built with the help of some incredible open-source projects, including:

- [Zed](https://github.com/zed-industries/zed) — a wonderful editor (~~ab~~)used by all core team members. Conveniently provides `gpui` — their native Rust rendering stack.
- [librespot](https://github.com/librespot-org/librespot) — Spotify playback and library integration.
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — certain YouTube ideas implemented in [ytmusic-rs](https://github.com/nolight132/ytmusic-rs). :)

## License

Copyright (C) 2026 Sonora Contributors.

Sonora is free software, released under the [GNU General Public License version
3 or later](COPYING).

Sonora is an unofficial client and is not affiliated with, endorsed by, or
sponsored by Spotify AB.

The binary also embeds the [Inter](https://github.com/rsms/inter) typeface (SIL
Open Font License 1.1) and the [Lucide](https://lucide.dev) icon set (ISC
License). `THIRD-PARTY.md` lists every bundled dependency.
