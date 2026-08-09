# sonora

A native Spotify client in Rust on [GPUI](https://github.com/zed-industries/zed) (Zed's UI
framework), streaming through [librespot](https://github.com/librespot-org/librespot).
Linux-only today. Cargo workspace, edition 2024, resolver 3.

## Read this before writing code

1. **A component probably already exists.** Check `crates/ui/src/lib.rs`, `crates/views/src/cells.rs`
   and the tables below before writing a new element. See [Before you build a component](#before-you-build-a-component).
2. **Never call the Spotify Web API.** There is no `reqwest` call to `api.spotify.com` anywhere and
   there must not be one. All data comes from librespot's `spclient`. See [Backend](#backend-how-data-actually-arrives).
3. **Never hardcode a color, radius, or size.** Everything comes from `cx.theme()`.
   See [Theme and metrics](#theme-and-metrics).
4. **Network work runs on the tokio runtime (`Io`), never on GPUI's executor.**
   See [Async: two runtimes](#async-two-runtimes).
5. **New assets must be registered in `crates/sonora/src/assets.rs`** or they silently fail to load.

## Crate layout

```
crates/
  sonora/     binary: main, window, actions, asset registry, HTTP client shim
  views/      screens plus app chrome: title bar, sidebar, player bar, filter/search field
  state/      GPUI entities holding app state; owns all async orchestration
  spotify/    Spotify data access over librespot spclient (no GPUI)
  audio/      librespot playback engine + custom rodio sink (no GPUI)
  ui/         design system: theme, metrics, and reusable elements (gpui only)
  router/     Destination enum, navigation history, Link trait
  input/      text input element + global actions and keybindings
  i18n/       Fluent localization: the `t!` macro, locale selection, embedded .ftl
```

Dependency direction is strict; do not create a back edge:

```
sonora → views → state → spotify
                       → audio
         all ui-side crates → ui, router, input → ui → gpui
         every ui-side crate → i18n → gpui
```

- `ui` depends only on `gpui`, `serde` and `i18n`. It must never know about `spotify`, `state`, or
  playback.
- `spotify` and `audio` must never depend on `gpui`. They are plain async Rust.
- `state` depends on `ui` only for `ThemeOverrides`, `MIN_FONT`, `MAX_FONT` (settings persistence).
- Widgets that need app state (player bar, sidebar) live in `views/src/chrome/`, not `ui`.
- `i18n` is a leaf: it depends on `fluent-bundle`, `unic-langid`, `sys-locale` and `gpui` (for
  `SharedString`) and on nothing else in the workspace.

## Building

### Any platform: what the build needs

The GPUI renderer is Vulkan-based, so a Vulkan ICD is a **runtime** requirement, not just a build
one. Link-time deps: `vulkan-loader`, `wayland`, `libxkbcommon`, `libxcb`, `libx11`, `libxcursor`,
`libxi`, `fontconfig`, `freetype`, `alsa-lib`, plus `pkg-config`.

`.cargo/config.toml` passes `-fuse-ld=mold` for `x86_64-unknown-linux-gnu`, so **mold must be on
PATH** for that target. If it isn't, either install mold or build with
`RUSTFLAGS="" cargo build …` to drop the flag.

### Nix (primary)

```sh
nix develop          # or: direnv allow  (.envrc runs `use flake`)
cargo run --locked --package sonora
nix run              # build + run the packaged binary
nix build            # ./result/bin/sonora
```

The devShell supplies `rustc`, `rustfmt`, `rust-analyzer`, `mold`, `pkg-config`, `sccache` and the
runtime libs via `LD_LIBRARY_PATH`. It does **not** ship `cargo` or `cargo-clippy` — those come from
the ambient system profile here. If `cargo` is missing inside the shell, that's why.

When `Cargo.lock` changes, `cargoHash` in `flake.nix` goes stale. Build once, take the `got:` hash
from the failure, and paste it in.

### Arch / CachyOS

```sh
sudo pacman -S --needed base-devel rust pkgconf alsa-lib fontconfig freetype2 \
  libx11 libxcb libxcursor libxi libxkbcommon libxkbcommon-x11 wayland \
  vulkan-icd-loader mold
# plus a Vulkan driver: vulkan-radeon | vulkan-intel | nvidia-utils

cargo run --locked --package sonora
cargo build --release --locked --package sonora && ./target/release/sonora
```

### Debian/Ubuntu and Fedora

Not exercised in this repo; package sets translated from the dependency list above.

```sh
# Debian/Ubuntu
sudo apt install build-essential pkg-config mold libasound2-dev libfontconfig1-dev \
  libfreetype-dev libx11-dev libxcb1-dev libxcursor-dev libxi-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev mesa-vulkan-drivers

# Fedora
sudo dnf install @development-tools pkgconf-pkg-config mold alsa-lib-devel fontconfig-devel \
  freetype-devel libX11-devel libxcb-devel libXcursor-devel libXi-devel \
  libxkbcommon-devel libxkbcommon-x11-devel wayland-devel vulkan-loader-devel mesa-vulkan-drivers
```

### macOS / Windows

Released, but not developed against here. `.github/workflows/release.yml` builds both Apple targets
and `x86_64-pc-windows-msvc`; the `x11`/`wayland` features on `gpui_platform` are inert off Linux,
so the pin does no harm. There is no local toolchain for either — the release workflow is the only
thing that exercises them, and it only runs on a tag. The flake declares `x86_64-linux`/`aarch64-linux`
only, and the mold linker flag targets Linux.

macOS ships as a universal `Sonora.app` inside a disk image: `lipo` merges the two arch builds,
`codesign --force --sign -` signs it ad-hoc, `hdiutil` wraps it. Ad-hoc signing only makes the
binary runnable on Apple Silicon — it does not satisfy Gatekeeper, so an unnotarized build still
needs `xattr -dr com.apple.quarantine` on first launch. Do not add `--deep`; Apple deprecates it for
signing and the bundle has no nested code.

Windows embeds `assets/windows/sonora.ico` through `crates/sonora/build.rs` and `winresource`.

### Checks

```sh
cargo fmt                      # rustfmt.toml: edition 2024, style_edition 2024
cargo check --workspace
cargo test --workspace
cargo clippy --workspace       # lib targets
```

The first build compiles GPUI from source; expect several minutes.

`cargo clippy --workspace --all-targets` currently **fails**, on a deny-level
`reversed_empty_ranges` from a deliberate `clamp_range("abc", &(2..1))` case in
`crates/input/src/text.rs`. That failure predates your change; don't "fix" the test to silence it.
The workspace also carries a handful of clippy warnings (complex types, a missing `Default`). Leave
them unless the task is about them.

### Runtime environment

|                   |                                                                    |
| ----------------- | ------------------------------------------------------------------ |
| Settings          | `$XDG_CONFIG_HOME/sonora/settings.json`                            |
| Credentials cache | `$XDG_CACHE_HOME/sonora/credentials.json`                          |
| OAuth redirect    | `http://127.0.0.1:8989/login`, override with `SONORA_REDIRECT_URI` |
| Logging           | `RUST_LOG`; default filter `warn,symphonia=error`                  |

## Before you build a component

Grep first. In order: `crates/ui/src/lib.rs` (exports), `crates/views/src/cells.rs` (grid cell
renderers), `crates/views/src/chrome/` (chrome). Extend what's there — add a builder method to
`Button` rather than writing `IconButton`.

### `ui` — reusable elements

| Item                                                                    | Use for                                                                                                                                |
| ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `Button`                                                                | every button. Variants `.ghost() .outline() .primary() .danger()`, plus `.small() .icon() .label() .tint() .selected() .disabled()`    |
| `Card`                                                                  | artwork + title + eyebrow + meta row/tile. `.art()` for tile mode, `.circle()`, `.loading()`, `.trailing()`, `.explicit()`, `.press()` |
| `Artwork`                                                               | cover images with skeleton loading and a music-note fallback                                                                           |
| `Skeleton`, `Initials`                                                  | pulsing loading placeholder; avatar initials                                                                                           |
| `Grid`, `GridSource`, `GridDelegate`, `GridState`, `ColumnSpec`, `Cell` | every table. Virtualized, sortable, filterable, hideable columns, responsive `hide_below`                                              |
| `Scroller` + `Scrollbar`                                                | any scrolling region. Do not use bare `overflow_y_scroll`                                                                              |
| `Scrubber` + `ScrubberState`                                            | any draggable 0..1 track (seek bar, volume)                                                                                            |
| `Panel` + `Side`                                                        | a resizable side panel shell: clamped width, drag grip, pixel snapping. `.limits()`, `.fill()`, `.on_resize()`                         |
| `Menu`, `MenuItem`                                                      | dropdowns (deferred + occluded, with `on_dismiss`)                                                                                     |
| `InlineLinks`, `InlineLink`                                             | comma-joined clickable artist lists                                                                                                    |
| `eyebrow()`, `heading()`                                                | the two standard text styles                                                                                                           |
| `ExplicitBadge`                                                         | the "E" badge                                                                                                                          |
| `WindowControls`                                                        | minimize/maximize/close, honoring platform decorations                                                                                 |
| `clock()`                                                               | `Duration` → `m:ss`                                                                                                                    |
| `snapped()`                                                             | round a `Pixels` to the device pixel grid                                                                                              |

Also available: `Input` (`input` crate — full text editing, IME, selection, clipboard) and
`Link` (`router` — makes a `Stateful<Div>` navigate on click).

### `views/src/cells.rs` — grid cell renderers

`index` (play/pause/now-playing transport with hover preload), `artists`, `link`, `text`, `dim`,
`title`, `artwork`, `blank`, `transport`, `toggle`, `artist_links`. Reuse these in any new
`GridSource::cell`.

### Element conventions

New elements follow one shape — copy `ui/src/button.rs`:

```rust
#[derive(IntoElement)]
pub struct Thing { base: Stateful<Div>, /* … */ }

impl Thing {
    #[track_caller]
    pub fn new(id: impl Into<ElementId>) -> Self { … }
    pub fn variant(mut self) -> Self { …; self }   // consuming builders
}

impl Styled for Thing { fn style(&mut self) -> &mut StyleRefinement { self.base.style() } }
impl InteractiveElement for Thing { … }

impl RenderOnce for Thing {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let overrides = std::mem::take(base.style());   // caller styles win
        let mut thing = base./* defaults */;
        thing.style().refine(&overrides);
        thing
    }
}
```

The `mem::take` / `refine` dance is deliberate: it lets a call site override any default without
the element re-applying it afterwards. Keep it.

Stateful components (`Scrollbar`, `Input`, `GridState`) are `Render` entities instead, created with
`cx.new(…)` and held by the parent.

## Theme and metrics

`ui/src/theme.rs` is the single source of every color, radius, and font size.

```rust
use ui::ActiveTheme as _;
let theme = *cx.theme();          // Theme is Copy
theme.foreground, theme.muted_foreground, theme.secondary_hover, theme.table_row_border, …
theme.radius
theme.text(Text::Small)           // Tiny Small Label Body Large Title Display
theme.metrics.row / .header / .pad / .inset / .control / .field
             .title_bar / .player_bar / .list_row / .thumb / .cover
```

- Every metric scales off the user's font size (`Metrics::new(base)`), so **never** hardcode a
  height for a row, control, or bar — read it from `theme.metrics`.
- A literal `px(…)` is acceptable only for a local, non-scaling detail, declared as a `const` at
  module top (see `NUMBER`, `DATE` in `views/src/cells.rs`). Responsive thresholds are **not** a
  local detail — see [Breakpoints and content width](#breakpoints-and-content-width).
- Adding a color token means adding it to `Theme`, to every `Theme::*()` constructor, to
  `ThemeOverrides`, and to the `apply_color!` list — all four, or overrides break.
- Eight themes exist (`ThemeKind::ALL`). `ocean`/`rose`/`lavender`/`amber` are derived by mutating
  `midnight()`/`dark()`; follow that pattern rather than writing a full palette.
- Users can override any token via `settings.json`; `Theme::set` re-renders all windows.

## Localization

Every user-facing string comes from Fluent. **Never render a bare English literal**; add a key to
`assets/i18n/en-US/main.ftl` and translate it in `ru`, `uk` and `pl` — a test in `crates/i18n`
fails if the key sets diverge.

```rust
use i18n::t;

t!("song-credits")                                   // -> SharedString
t!("song-disc-track", disc = disc, track = number)   // named args, any number or &str
i18n::lookup(key, None)                              // when the key is a runtime &str
```

- The `.ftl` files are embedded with `include_str!` in `crates/i18n/src/language.rs`, so they do
  **not** go through `crates/sonora/src/assets.rs`.
- Keys are scoped by area: `nav-`, `column-`, `menu-`, `queue-`, `player-`, `search-`, `song-`,
  `settings-`, `theme-`, `common-`. Values stay natural case — `ui::eyebrow()` / `ui::upper()`
  do the shouting.
- Counts use Fluent selectors (`{ $count -> [one] … [few] … *[other] … }`), never string
  concatenation; ru/uk/pl need `one`/`few`/`other` to be right.
- Dates are assembled from `month-1`..`month-12` plus `date-full`, never `strftime`.
- A missing key logs `i18n: … is missing` and falls back to English, then to the key itself.
- `ColumnSpec::header` and `Slot::Header` hold a **key**, not a label; call `ColumnSpec::label()`.
- **Never call `t!` in a constructor.** Anything stored on a struct and rendered later must hold the
  key and resolve in `render`, or it freezes in whatever language was active at construction and
  never follows a language change. `Input::new`/`set_hint` and `Searchable::hint()` take keys for
  exactly this reason.
- Developer-facing text — `.context("cannot …")`, `log::warn!`, `PlaybackState::Failed` — stays
  in English. So do wire values in `crates/spotify`.
- The language lives in `settings.json` (`language`, default `"auto"`). Changing it calls
  `i18n::set` then `cx.refresh_windows()`, the same way `Theme::set` repaints.

## Breakpoints and content width

Every responsive decision in the app uses one ladder, defined in `ui/src/layout.rs`. **Never write a
bare `px(…)` breakpoint** — no `width < px(640.)`, no per-view `NARROW`/`STACK_BREAKPOINT` const.

```rust
use ui::{Room, ALWAYS, SNUG, ROOMY, WIDE, VAST};   // 0 · 420 · 620 · 740 · 1180

Room::of(width)              // Tight | Snug | Roomy | Wide | Vast
Room::of(width).fits(Room::Roomy)   // ">= Roomy", the only comparison you need
```

`Room` is `Ord`, so `fits` is just `>=`. The raw `Pixels` consts exist for the places that need a
number rather than a step: `ColumnSpec::hide_below` and centering maths.

A breakpoint is a branch — "below this width, lay the page out differently". A packing minimum is
not: `MIN_COLUMN_WIDTH` in `home/quick_picks.rs`, `PANEL` in `screens/song.rs`, the column widths in
`shared/cells.rs`. Those stay plain `px` consts at module top; forcing them onto a five-step ladder
would change how content packs.

Two modules own the measurement:

- **`ui::layout`** — the ladder itself. Pure `gpui`; knows nothing about panels.
- **`views::chrome::Chrome`** — how much horizontal room content actually has, after the left and
  right sidebars take their cut.

```rust
use crate::chrome::Chrome;
Chrome::content(window, cx)   // viewport width − both sidebars, floored at ui::MIN_CONTENT
Chrome::room(window, cx)      // the same, classified into a Room
chrome::cap(min, max, window) // a panel's ceiling: never eat ui::MIN_CONTENT
```

`Chrome::room` is how a view classifies its width: never rebuild `Room::of(…)` from a width you
derived yourself.

Rules:

- **Measure against `Chrome::content`, never `window.viewport_size().width`.** A raw viewport width
  ignores both side panels, so tables overflow under the right sidebar and grids pick a column count
  that does not fit. Raw viewport width is legitimate in exactly two places: chrome that spans the
  whole window (`PlayerBar`, `TitleBar`, and `Menu`'s submenu flip), and a side panel deciding *its
  own* size. `Chrome::content` would be circular for a panel, because the panel is a term in it.
- **One panel never sets another's width.** Each clamps itself against the viewport through
  `chrome::cap`, so neither can squeeze content below `ui::MIN_CONTENT` on its own, and neither
  panel's size is ever a function of the other's.
- **Hiding is a different question from sizing, and it does look at both panels.** `SidebarLeft`
  auto-hides once the room left for content — viewport minus its own width *and*
  `Chrome::sidebar_right` — drops below `Room::Wide`, so opening the queue pushes it out of the way
  without resizing it. It reads last frame's publish, which cannot loop: the decision changes
  visibility, never width. `SidebarRight` decides its own takeover from the viewport alone.
- **A panel's width changes only when the user drags it.** `chrome::cap` feeds `Panel::reach`, which
  bounds the *drag*, not the render — narrowing the window must never silently shrink a panel the
  user sized. `Panel::limits` stays the static floor and ceiling.
- `Workspace::render` publishes the widths every frame via `Chrome::publish`; it notifies only when
  a width actually changed, so it cannot loop.
- **`Chrome` is only current after that publish.** `Workspace` renders *before* it and owns both
  panels, so it reads them directly; everything it renders into — content views and both panels —
  sees this frame's values.
- **A view whose layout depends on width must observe the chrome**, or it will not repaint when a
  panel is resized or toggled:
  ```rust
  let chrome = Chrome::entity(cx);
  cx.observe(&chrome, |_, _, cx| cx.notify()).detach();
  ```
  Nothing outside `Workspace` holds an `Entity<SidebarLeft>` — that is what `Chrome` replaced.
- `views::cells::content_width(window, inset, cx)` is the shortcut for grid pages: `Chrome::content`
  minus the page's own padding.

Both side panels are built on `ui::Panel` (`Side::Left` / `Side::Right`), which owns the width
clamping, the drag grip and the snap to the device pixel grid; each reports its new width through
`on_resize` and persists it in `settings.json` (`sidebar_width`, `sidebar_right_width`).

## Async: two runtimes

GPUI has its own executor; librespot and `reqwest` need tokio. `state::Io` wraps a multi-thread
tokio `Runtime` and is a GPUI global.

```rust
let io = Io::global(cx);
self.task = Some(cx.spawn(async move |this, cx| {      // GPUI executor
    let loaded = join(io.spawn(async move {            // tokio: all network work
        client.album_tracks(&id).await
    })).await;
    this.update(cx, |this, cx| { /* apply, cx.notify() */ }).ok();
}));
```

Rules:

- Anything touching `SpotifyApi`, librespot, or sockets goes inside `io.spawn`.
- Only mutate entity state inside `this.update`, and end with `cx.notify()`.
- Store the returned `Task` in a field (`task`, `load`, `fetch`) — dropping it cancels the work,
  which is how sign-out and navigation cancel in-flight loads. Never `.detach()` a data load.
- `join(handle)` (crate-private in `state`) flattens `JoinHandle<Result<T>>`.
- `cx.subscribe(…)` / `cx.observe(…)` **are** `.detach()`-ed, in the constructor.
- Because `join` and `Io` plumbing live in `state`, new network-backed features belong in a `state`
  entity — not in a view.

## Backend: how data actually arrives

### There is no Spotify Web API here

`crates/spotify` talks to Spotify through `librespot_core::Session::spclient()` — the same internal
endpoints the official client uses, returning protobuf or JSON. Consequences:

- Do not add `reqwest` calls to `api.spotify.com`, do not add a client secret, do not add
  `rspotify`. The only `reqwest` in the tree is `crates/sonora/src/http.rs`, a `gpui::HttpClient`
  adapter that exists purely so GPUI can fetch cover images.
- Auth is OAuth PKCE via `librespot-oauth` against **Spotify's own client id**
  (`DEFAULT_CLIENT_ID` in `auth.rs`). A developer-app client id will be refused at session connect —
  `auth::denied` documents this. Don't "fix" auth by swapping in a registered app id.

### Module map

| Module                    | Endpoint / mechanism                                                              |
| ------------------------- | --------------------------------------------------------------------------------- |
| `auth.rs`                 | OAuth login, credential cache, `restore` / `login` / `forget`                     |
| `client.rs`               | `SpotifyApi` trait + `LibrespotClient`; the only type views/state see             |
| `collection.rs`           | saved tracks; `metadata()` — batched `get_extended_metadata` (TRACK_V4)           |
| `collection2.rs`          | `/collection/v2/paging` — hand-rolled protobuf, paged, honors tombstones          |
| `albums.rs`, `artists.rs` | extended metadata for albums/artists, artist portraits                            |
| `playlists.rs`            | `get_playlist` → `SelectedListContent` → uri list → `collection::metadata`        |
| `search.rs`               | `get_context("spotify:search:…")` → track uris → `collection::metadata`           |
| `radio.rs`                | `get_radio_for_track` → a generated playlist id → `playlist_tracks`               |
| `profiles.rs`             | display-name lookups, fanned out over a `JoinSet`                                 |
| `wire.rs`                 | protobuf → `models` conversion, `image_url` (file id → `i.scdn.co/image/<hex>`)   |
| `pb.rs`                   | minimal protobuf `Reader`/`Writer` for endpoints with no generated schema         |
| `models.rs`               | `Track`, `Album`, `AlbumDetail`, `Artist`, `ArtistRef`, `Playlist`, `UserProfile` |

Working notes:

- Prefer generated messages from `librespot-protocol`. `pb.rs` exists only because the collection-v2
  paging schema isn't in that crate — don't hand-roll a new one if a generated type exists.
- The recurring pattern is **uris → `collection::metadata` → `Track`**. A new track-listing endpoint
  should resolve uris and reuse `metadata`, not re-parse track fields.
- `Track::id` is `Option<String>` base62 (no `spotify:track:` prefix); the prefix is added at the
  audio boundary. `Track::playable` is false for unavailable tracks — check it before playing.
- New endpoints: add the method to the `SpotifyApi` trait, implement on `LibrespotClient` by
  delegating to a focused module. The trait exists so `state` depends on an interface, not on
  librespot directly.
- Errors use `anyhow` with `.context("cannot …")` in lowercase.

### Audio

`crates/audio` owns `librespot_playback::Player` plus `BlazingSink`, a custom rodio sink with
smooth gain ramping and flush-on-seek. `Engine::start(session, config)` returns
`(Engine, AudioEvents)`; `AudioEvents::next()` yields the reduced `AudioEvent`
(`Loading/Playing/Paused/Position/Ended/Unavailable`).

Never drive librespot's `Player` from a view. Go through `state::Playback`, which owns the engine,
pumps events into `PlaybackState`, and handles shuffle, repeat, skip debouncing, and the cooldown
after an `Unavailable` track.

### State entities

`state::init` installs a `Sonora` global holding `session`, `library`, `playback`, `queue`,
`settings`. Reach them with `Sonora::global(cx)`.

| Entity                                     | Responsibility                                                                                                                                          |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Session`                                  | auth lifecycle; emits `SessionEvent::{SignedIn, SignedOut}`; hands out `Arc<dyn SpotifyApi>` and the raw librespot session                              |
| `Library`                                  | saved tracks / playlists / albums; `LibraryState` is `Empty \| Loading \| Ready{..,problems} \| Failed` — partial failure is normal, surface `problems` |
| `Playback`                                 | engine ownership, transport, shuffle/repeat, volume, `Origin` tracking                                                                                  |
| `Queue`                                    | past / current / upcoming; `start`, `next`, `next_random`, `previous`, `rewind`                                                                         |
| `Home`, `Detail`, `ArtistDetail`, `Search` | per-screen loaders, each owning its `Task`                                                                                                              |
| `AppSettings`                              | debounced JSON persistence                                                                                                                              |

Everything reacts to `SessionEvent`: signing out must clear derived state. If you add an entity that
caches Spotify data, subscribe to `Session` and clear on `SignedOut`.

## UI patterns

**Navigation.** `router::Destination` is the route enum; `router::navigate(dest, cx)`, `back`,
`forward`. `Root` subscribes to `NavigationEvent::Moved` and swaps the workspace content. For a
clickable region, use the `Link` trait (`div().id(..).link(Destination::Album(id))`) instead of a
manual click handler.

**Shells.** `crates/views/src/shells/` holds the two top-level layouts, `Workspace` and
`FullscreenView`; `Root` swaps between them. A shell owns its own chrome — `Workspace` builds both
sidebars and the player bar — and answers for its title bar through the `shells::Shell` trait
(`title_bar(content, cx) -> TitleBarOptions`). `Root` supplies only the current screen's toolbar and
asks the active shell; it never reaches into a panel.

**New screen checklist:** add a `Destination` variant → add a state entity if it loads data → add
the view under `crates/views/src/` → construct it in `Root::new` and wire it in `Root::show` →
add a sidebar entry in `views/src/chrome/sidebar_left.rs` if it's top-level.

**Tables.** Implement `GridSource` (`columns`, `rows`, `cell`, and optionally `compare`, `matches`,
`playing`, `is_loading`), define a `&'static [ColumnSpec<Field>]`, hold a
`GridState<Source>` entity, render `grid(&state)`. Column widths use `Width::{Fixed, Fill, Thumb}`
and `hide_below` for responsive dropping. Hidden columns persist through
`AppSettings::{hidden_columns, set_hidden_columns}`.

**Toolbar.** `chrome::Toolbar` owns only the search field and lays out whatever a screen hands it.
A screen implements `chrome::Tooled` — `toolbar()` returns its `Entity<Toolbar>`, `tools(&self, cx)`
returns the finished `Vec<AnyElement>` — and wires itself once with `Toolbar::wire`. Adding a
control to a screen never touches `toolbar.rs`. Build the standard controls with the shared builders
in `chrome::tools` (`columns`, `filters`, `sorts`, `views`) rather than writing a menu twice; each
screen owns one `ui::Popovers` so only one of its popovers is open at a time, and holds its own
`tools::Sliders` cache so scrubber positions survive across frames (`LibraryView` keeps one per
section, so tab switches cannot bleed).

**Filtering.** Implement `chrome::Searchable` on the view; `Toolbar::bind` binds it to the search
field in the title bar. Don't build a second search box.

**Actions and keys.** Declare actions in `crates/input/src/lib.rs` (`actions!` macro), bind them in
`bindings()`, handle them with `cx.on_action` (global, in `sonora/src/actions.rs`) or
`.on_action(cx.listener(…))` (scoped). Key contexts: `Workspace`, `Input`, `Grid`. Both `cmd-` and
`ctrl-` bindings are registered for every shortcut.

**Assets.** SVGs live in `assets/icons/`, are embedded with `include_bytes!`, and are referenced as
`"icons/<name>.svg"`. Adding a file is not enough — add the stem to the `ICONS` list in
`crates/sonora/src/assets.rs`, otherwise loading logs `assets: … is not registered` and renders
nothing. Icons are Lucide (`assets/icons/LICENSE`); the UI font is Inter.

**App icons are generated, never hand-edited.** `assets/icon.svg` is the master; `scripts/generate-icons.py`
derives every platform artefact from it — a circle for `assets/linux/` (scalable SVG plus the hicolor
PNG set), an Apple squircle for `assets/macos/sonora.icns`, and a rounded rect for
`assets/windows/sonora.ico`. Change the master and re-run the script; do not touch the outputs.

**`THIRD-PARTY.md` is generated too.** `scripts/generate-notices.py` drives `cargo about` over
`about.toml` and writes the file; a new dependency means re-running it, not editing the output. The
binary must ship it alongside `LICENSE`, `assets/fonts/LICENSE.txt` and `assets/icons/LICENSE` —
`flake.nix` and `.github/workflows/release.yml` both do that, and a package that skips it is
distributing unlicensed code.

## Code style

- **Comments: essentially none.** The codebase has almost zero. Name things so they don't need one.
  If a comment is unavoidable it is at most three lowercase words, no trailing period.
- Every `.rs` file opens with `// SPDX-License-Identifier: GPL-3.0-or-later`; that line is the one
  comment exempt from the rule above.
- Names are short and domain-flavored: `Look`, `Metrics`, `Grab`, `Scored`, `wire`, `pb`, `clock`,
  `snapped`. Prefer a noun that reads at the call site over a descriptive compound.
- `use gpui::prelude::*;` then explicit imports; traits imported anonymously (`use ui::ActiveTheme as _;`).
- Module shape: `use` block → `const`s in SCREAMING_SNAKE → types → impls → private free helpers at
  the bottom.
- Prefer combinator chaining over branching in render: `.when()`, `.when_some()`, `.when_else()`,
  `.map()`. Prefer `match` over `if/else` for two-arm boolean choices — that's the house style
  (`match small { true => …, false => … }`).
- Use let-else for early exits; return early rather than nesting.
- `anyhow::Result` at boundaries, `.context("cannot …")` lowercase; log with `log::warn!`/`error!`
  prefixed by subsystem (`"playback: …"`, `"settings: …"`, `"assets: …"`).
- Tests are `#[cfg(test)] mod tests` at the bottom of the file they cover — see
  `spotify/src/{collection2,radio,search}.rs` and `input/src/text.rs`. They're pure-function tests;
  there is no UI or network test harness.
- Dependencies go in the root `[workspace.dependencies]`, then `dep.workspace = true` in the crate.
  `gpui`/`gpui_platform` are pinned to one git rev — bump both together or the build breaks.

## Commits

Conventional Commits: `type(scope): description`, imperative, lowercase, no trailing period, no body.
Scopes in use: `player`, `playback`, `views`, `ui`. Never add a `Co-Authored-By` trailer or any
assistant attribution.

## Pull requests

One heading, one list. Nothing else:

```markdown
## Summary

- add playlist create, rename, delete, visibility, and track mutation support
- add album and playlist queue actions
- add playlist management menus and localized editor UI
```

Bullets are lowercase, imperative, one line each, and describe behaviour rather than files. No
Why/User impact/Validation sections, no checklists, no screenshot boilerplate, no test plan. This
overrides any wider PR template. Never sign off or attribute the assistant.
