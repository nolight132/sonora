# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Sonora is on the AUR: `sonora-bin` installs the released binary, `sonora` builds it from source.

## [0.9.0] - 2026-08-11

### Added

- Sonora can play the music already on this device: point it at a folder in the settings and its
  files show up as tracks, albums and artists next to the streaming services.
- The right sidebar shows the lyrics of the playing track next to the queue, switched with a pair of
  floating pills at its foot. Timed lyrics highlight the line being sung, scroll along with it, and
  jump playback to a line when it is clicked.

### Changed

- The right sidebar is opened from the toolbar rather than the player bar, and the player bar button
  in its place opens fullscreen instead.
- A narrow window drops the right sidebar entirely rather than letting it cover the page; lyrics and
  the queue live in fullscreen there.

## [0.8.0] - 2026-08-11

### Added

- Sonora can now sign in to YouTube Music as an alternative to Spotify. The login screen offers
  both services; YouTube Music can be browsed as a guest, connected by importing an existing
  browser session, or connected by pasting cookies, and the whole library — liked songs,
  playlists, albums, artists, search, and radio — works through the same interface.
- The general settings gain a "Manage accounts" section: every service is listed with its own sign
  out, switching to a service already connected takes effect immediately, and a service that is not
  connected yet offers its sign-in options right there — including importing from a named browser.
- The login screen puts each service in its own column with guest mode below them, and importing a
  YouTube Music session asks which browser to read it from — Firefox, Zen, LibreWolf, Floorp,
  Waterfox, Mullvad, Tor, Pale Moon, Basilisk, SeaMonkey, Chrome, Chromium, Brave, Edge, Vivaldi,
  Opera, Yandex, Arc, Thorium and Helium are recognised, including Flatpak and Snap installs.
- YouTube Music albums and playlists play the album audio of a song rather than its music video, so
  a track lasts as long as its listed length.
- Switching, pausing, resuming, and seeking a YouTube Music track fade in and out instead of
  clicking, the previous track stops the moment a new one is picked, and the transport stays
  responsive while the next track loads.

### Changed

- Release cards on an artist page show the release year instead of repeating the artist's name.

### Fixed

- A disabled button reads clearly instead of fading its label into its own background.
- Pasting YouTube Music cookies happens in a dialog that can be dismissed, so a sign-in started by
  mistake no longer leaves the login screen waiting for input.
- Signing out of one service switches to another service still connected, and only falls back to the
  login screen once nothing is connected.
- YouTube Music playlists you own are recognised as yours, so renaming, changing visibility and
  deleting are offered instead of an unsupported "remove from library" that always failed.
- Creating a YouTube Music playlist reports success instead of an error, and the new playlist
  appears straight away rather than after a refresh.
- Removing a track from a YouTube Music playlist works for tracks whose music video was swapped for
  its album audio.
- A YouTube Music playlist you own no longer lists its privacy setting as its owner.
- The player switches to the next YouTube Music track the moment the previous one ends, instead of
  lagging up to half a second behind the audio.
- A YouTube Music session whose saved cookies stopped working falls back to guest browsing or the
  login screen instead of failing with an error.

## [0.7.0] - 2026-08-10

### Added

- Sonora keeps a log file at `$XDG_STATE_HOME/sonora/sonora.log`, so a problem noticed after hours of
  use can still be diagnosed without having started the app from a terminal.
- Playback failures explain themselves: a track that cannot be played is named in a toast, and if
  Spotify refuses the account playback keys entirely, Sonora says so once and stops instead of
  failing through track after track.

### Fixed

- Sonora holds on to far less memory during long listening sessions: cover art it has not shown for
  a while is released instead of being kept until the app closes, and cover art whose page was left
  before the download finished no longer stays in memory for the rest of the session.
- The left sidebar keeps the width you gave it. A window too narrow to fit it now hides it and brings
  it back at that same width, instead of squeezing it down to its narrowest and forgetting the width
  you had; it can still be resized while it sits over the content.
- Card views stay responsive in large libraries: album, song and playlist grids now draw only the
  rows on screen, so resizing the window no longer stutters or reloads covers, and an artist's
  discography opens without a pause.

## [0.6.0] - 2026-08-10

### Added

- Sonora answers the system media controls: media keys, the desktop's now-playing widget and
  lock-screen controls can play, pause, skip, seek and set the volume, and they show the current
  track with its cover art.
- Spotify links open in Sonora: a `spotify:` link to a track, album, playlist or artist opens that
  page, handing it to the window already running instead of starting a second one.

### Changed

- The app presents itself as Sonora rather than sonora, in the window title, the application menu
  and the Windows file properties.

## [0.5.0] - 2026-08-09

### Added

- Albums and playlists can be added to and removed from the library, from their context menu and
  from a heart button beside Play on their page.
- A transparent window background, with an opacity slider under Appearance settings.
- Settings are split into General, Appearance, Playback and About tabs, reachable from the sidebar.
- Copy link actions for tracks, albums, playlists and artists.
- Monthly listeners on the artist page, and play counts beside an artist's popular tracks.
- Toasts confirming queue and playlist additions.
- The volume can be set by scrolling the mouse wheel over the volume control.
- Turning on radio appends what it will play to the queue and lists it under a Similar tracks
  section, picked from the last track you queued. Those tracks play, reorder and can be removed like
  any other queue entry.
- Tooltips on icon-only controls across the player, title bar, toolbar and queue.
- The next track is fetched before the current one ends, so it starts without a gap.
- Gapless playback, on by default and switchable under Playback settings: an album runs from one
  track into the next the way it was sequenced, instead of being cut at the track boundary.

### Changed

- Settings that are on or off are switches instead of text buttons.
- Context menu entries are grouped into sections separated by a rule.
- The sidebar always collapses when the window gets narrow; the setting that governed it is gone.
- Skipping quickly through several tracks only loads the one you stop on.
- The seek and volume sliders have a taller grab area, so they are easier to hit without looking
  any thicker.
- Library cards are virtualized, artist release artwork loads only once it is scrolled to, and the
  artwork image cache is bounded, so large libraries stay responsive.
- Headings, quick pick titles and queue track names are lighter and no longer bold.
- Releases ship static Inter faces instead of the variable font.
- Song, artist and album pages load their details in fewer requests.

### Fixed

- Radio builds its set of similar tracks once, instead of replacing it on every track it plays.
- Durations in search results stay on one line.
- Library cards keep their scroll position when you come back to them.
- The settings menu in the sidebar folds away when you navigate off settings.
- The Play button on a page plays the list as it is shown, so filtering or sorting no longer starts
  a track that is not in view.
- An empty library section says so instead of showing a bare table, and a filter that matches
  nothing says that too.
- Quick picks asks you to like a few songs once the library has loaded, instead of pulsing
  placeholders forever.
- The no-matches note in search lines up with the results above it.
- Clicking a dropdown trigger below the title bar closes its menu instead of leaving it open.
- Releases on the artist page answer to a right-click with the album menu.
- The percentage and timecode bubbles no longer appear below a slider, where a click would not land.
- Rows in quick picks, the queue and search results are inset by the same amount on all four sides,
  instead of drifting a pixel between the top and the bottom.
- Timestamps an hour or longer carry an hours field instead of overflowing the minutes.
- Track menus no longer offer a link to the page that is already open.
- A right-click on a table row no longer also opens the menu behind it.
- Artist release artwork no longer disappears while the page scrolls.
- The library card grid is padded on both sides.
- Play counts of zero are treated as unknown rather than shown as zero.

## [0.4.1] - 2026-08-09

### Added

- An About page crediting the team.

### Changed

- Shuffle, repeat and whether the queue panel is open are remembered between sessions.

### Fixed

- Play next inserts after the current track instead of appending to the queue.
- Resizing one side panel no longer resizes the other.
- A panel responds only to its own drag grip.
- A button label truncates instead of overflowing its button.
- Long song metadata is contained rather than spilling out of its row.

## [0.4.0] - 2026-08-09

### Added

- Playlist management: create, rename, delete, change visibility, and add or remove tracks.
- Albums and playlists can be queued as a whole from their menus.
- Library cards are laid out on a grid that adapts its column count to the space available.
- Toasts above the player report playlist changes.

### Changed

- Item menus are built from one shared definition rather than per-screen copies.
- Track columns are composed from one shared set across every table.
- Playlist edits update the library in place instead of reloading it.
- The library Songs tab stays in list mode, where a card grid adds nothing.
- The text input moved into the design system, and side panels are built on one panel primitive.

### Fixed

- Radio started from a search result is seeded with the track that was picked.
- A newly created playlist is named on creation instead of appearing untitled.

## [0.3.0] - 2026-08-08

### Added

- Columns can be reordered by dragging a header and resized by dragging its edge. Widths, order,
  hidden columns and the active sort are remembered per table between sessions.
- Library sections switch between the table and a card grid through a new View control, remembered
  per section.
- A sort control in the toolbar, so sorting works in card mode as well as in the table.
- Card mode groups rows under first-letter or year headings when the active sort is groupable.
- Album and playlist artwork carries a Play button that starts, pauses and resumes in place.
- Go to album and Go to artist in the track context menu; the latter opens a submenu when a track
  has several artists.
- Queued tracks gained clickable artists, a remove button on hover and the full track menu.
- Liked tracks, with like icons shown on tracks inside albums and playlists.

### Changed

- Track, album and playlist cards are now one primitive rather than four parallel implementations.
- Right-click context menus are a separate element from button dropdowns.
- Every icon was refreshed from Lucide 1.30.0, and the sidebar toggle moved to the panel-left family
  so its divider and arrow match a left-hand panel.
- Album cards show the artist instead of the release type.

### Fixed

- Column headers no longer overlap each other; a column either shows its full heading or is dropped.
- Clicking a menu trigger a second time closes the menu instead of reopening it.
- Selecting an option no longer dismisses the filters, columns or sort dropdown, so several options
  can be picked and the duration slider can be dragged.
- Right-clicking another row moves the context menu in one click.
- Submenus no longer block clicks across the whole window.
- Ghost and outline buttons show a visible hover on the player bar, which is painted in the same
  colour their hover used to be.

## [0.2.0] - 2026-08-07

### Added

- Added an About tab in settings carrying the copyright, the warranty disclaimer and links to the
  license and the source.
- Added `THIRD-PARTY.md`, listing every bundled dependency and the full text of every license.
  Packages and release archives now ship it alongside the Inter and Lucide license files.
- Added a Nix package that installs the prebuilt release binary instead of compiling GPUI.

### Changed

- **Licensing.** Sonora is now released under the GNU General Public License version 3 or later.
  Earlier releases carried no license file at all, which left them undistributable; GPUI depends on
  `zlog` and `ztracing` from the Zed repository, both GPL-3.0-or-later, so every binary ever built
  from this tree was already covered by the GPL. Versions 0.1.0 and 0.1.1 are therefore to be read
  as GPL-3.0-or-later as well.

### Fixed

- Spaced the sidebar tabs apart and drew menus on the popover surface.

## [0.1.1] - 2026-08-07

### Added

- Localized the interface with Fluent, shipping English, Russian, Ukrainian and Polish.
- Added a language setting that follows the system locale by default.
- Added an application icon for macOS, Linux and the disk image.

### Changed

- Releases now ship bare executables for Linux and Windows and a universal disk image for macOS.
- Shuffle moved into the queue, so the queue panel shows the order that will actually play.

### Fixed

- Capped the volume taper at unity gain; the top of the slider no longer clips.
- Aligned the scrubber thumb with the pointer across the whole track.

## [0.1.0] - 2026-08-07

Initial release: a native Spotify client with playback, an interactive queue, the saved library,
search, album, playlist, artist and song pages, context menus and adaptive theming.

[unreleased]: https://github.com/nolight132/sonora/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/nolight132/sonora/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/nolight132/sonora/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/nolight132/sonora/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/nolight132/sonora/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/nolight132/sonora/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/nolight132/sonora/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/nolight132/sonora/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/nolight132/sonora/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nolight132/sonora/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/nolight132/sonora/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/nolight132/sonora/releases/tag/v0.1.0
