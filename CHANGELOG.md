# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[unreleased]: https://github.com/nolight132/sonora/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/nolight132/sonora/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/nolight132/sonora/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/nolight132/sonora/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/nolight132/sonora/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nolight132/sonora/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/nolight132/sonora/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/nolight132/sonora/releases/tag/v0.1.0
