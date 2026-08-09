# common
common-on = On
common-off = Off
common-left = Left
common-right = Right
common-search = Search
common-unknown = Unknown
common-not-provided = Not provided
common-not-available = Not available
common-cancel = Cancel
common-save = Save
common-delete = Delete
common-play = Play
common-more = More
common-previous = Previous
common-next = Next
common-dismiss = Dismiss
number-group = { "," }

# navigation
nav-home = Home
nav-search = Search
nav-library = Your Library
nav-settings = Settings
nav-songs = Songs
nav-albums = Albums
nav-playlists = Playlists
nav-back = Back
nav-forward = Forward
nav-sidebar = Toggle sidebar
library-liked-songs = Liked Songs
library-play-liked-songs = Play

# app menu
app-refresh-library = Refresh Library
app-sign-out = Sign Out
app-quit = Quit

# table columns
column-index = #
column-title = Title
column-artist = Artist
column-album = Album
column-date-added = Date added
column-length = Length
column-plays = Plays
column-name = Name
column-owner = Owner
column-year = Year
column-tracks = Tracks

# track menu
menu-add-to-playlist = Add to playlist
menu-new-playlist = New playlist
menu-no-playlists = No playlists
menu-add-to-library = Add to Library
menu-remove-from-library = Remove from Library
menu-remove-from-playlist = Remove from playlist
menu-play-next = Play next
menu-add-to-queue = Add to queue
menu-song-radio = Go to song radio
menu-go-to-album = Go to album
menu-go-to-artist = Go to artist
menu-view-details = View details
menu-copy-link = Copy link
menu-remove-from-queue = Remove from queue
menu-open-playlist = Open playlist
menu-play-playlist = Play playlist
menu-rename-playlist = Rename playlist
menu-delete-playlist = Delete playlist
menu-remove-playlist-from-library = Remove from Library
menu-make-playlist-public = Make public
menu-make-playlist-private = Make private
menu-open-album = Open album
menu-play-album = Play album
menu-add-album-to-queue = Add album to queue

# playlist editor
playlist-name-placeholder = Playlist name
playlist-create-title = Create playlist
playlist-rename-title = Rename playlist
playlist-delete-title = Delete playlist
playlist-delete-confirm = Delete “{ $name }”? This cannot be undone.

# queue panel
queue-title = Queue
queue-history = History
queue-now-playing = Now playing
queue-up-next = Up next
queue-reset = Reset
queue-clear = Clear
queue-empty = Your queue is empty
queue-similar = Similar tracks
queue-radio = Autoplay similar tracks

# player bar
player-nothing-playing = Nothing playing
player-percent = { $value }%
player-shuffle = Shuffle
player-repeat = Repeat
player-repeat-all = Repeat all
player-repeat-one = Repeat one
player-mute = Mute
player-unmute = Unmute
player-previous = Previous track
player-next = Next track
player-fullscreen = Fullscreen

# filters
filter-library = Filter your library
filter-album = Filter album tracks
filter-reset = Reset filters
filter-duration = Duration
filter-year = Year
filter-explicit = Explicit only
filter-playable = Playable only

# view
view-list = List
view-cards = Cards

# toolbar
tool-columns = Columns
tool-sort = Sort
tool-filters = Filters

# login
login-signed-out = Sign in to load your Spotify library
login-restoring = Checking your saved session…
login-authorizing = Waiting for authorization in your browser…
login-signed-in = Signed in as { $name }
login-sign-in = Sign in with Spotify

# album and playlist pages
detail-album = Album
detail-playlist = Playlist
detail-play-album = Play album
detail-play-playlist = Play playlist

# play button
play-pause = Pause
play-resume = Resume
play-loading = Loading…

# artist page
artist-eyebrow = Artist
artist-monthly-listeners = { $count ->
    [one] { $value } monthly listener
   *[other] { $value } monthly listeners
}
artist-play = Play now
artist-popular = Popular
artist-releases = Releases
artist-filter-all = All
artist-filter-albums = Albums
artist-filter-singles = Singles
artist-filter-eps = EPs

# release kinds
release-album = Album
release-single = Single
release-compilation = Compilation
release-ep = EP
release-audiobook = Audiobook
release-podcast = Podcast
release-meta = { $year } • { $kind }

# home page
home-quick-picks = Quick picks
home-quick-picks-eyebrow = Start from a song

# search page
search-placeholder = What do you want to listen to?
search-best-match = Best match
search-no-matches = No matches
search-results = Results
search-songs = Songs
search-artists = Artists
search-albums = Albums
search-tagged = { $kind } · { $value }
search-saved =
    { $count ->
        [one] { $count } song in Library
       *[other] { $count } songs in Library
    }
kind-song = Song
kind-artist = Artist
kind-album = Album

# song page
song-eyebrow = Song
song-play = Play song
song-view-album = View album
song-loading = Loading song information…
song-about = About this song
song-album = Album
song-released = Released
song-streams = Streams
song-position = Position
song-label = Label
song-popularity = Popularity
song-popularity-value = { $value }%
song-disc-track = Disc { $disc }, track { $track }
song-track = Track { $track }
song-credits = Credits
song-performed-by = Performed by
song-details = Genres & details
song-genres = Genres
song-language = Language
song-content = Content
song-explicit = Explicit
song-clean = Clean
song-about-artist = About the artist
song-artist-fallback = Explore the artist's popular songs and releases.
song-copyright = © { $notice }

# song languages
language-ar = Arabic
language-de = German
language-en = English
language-es = Spanish
language-fr = French
language-hi = Hindi
language-it = Italian
language-ja = Japanese
language-ko = Korean
language-pt = Portuguese
language-ru = Russian
language-tr = Turkish
language-uk = Ukrainian
language-zh = Chinese
language-zxx = No linguistic content

# counts
count-songs =
    { $count ->
        [one] { $count } song
       *[other] { $count } songs
    }

# dates
date-full = { $month } { $day }, { $year }
month-1 = Jan
month-2 = Feb
month-3 = Mar
month-4 = Apr
month-5 = May
month-6 = Jun
month-7 = Jul
month-8 = Aug
month-9 = Sep
month-10 = Oct
month-11 = Nov
month-12 = Dec

# settings
settings-tab-appearance = Appearance
settings-tab-playback = Playback
settings-tab-account = Account
settings-theme = Theme
settings-theme-detail = Choose the application colour palette
settings-theme-config = Open config
settings-adaptive = Adaptive theme
settings-adaptive-detail = Tint the palette with the artwork of the playing album
settings-corners = Corners
settings-corners-detail = How rounded surfaces and controls are
settings-font = Font size
settings-font-detail = Base text size, everything else scales with it
settings-font-value = { $size } px
settings-language = Language
settings-language-detail = The language sonora uses across the interface
settings-language-system = System
settings-auto-hide = Auto-hide sidebar
settings-auto-hide-detail = Collapse the sidebar when the window gets narrow
settings-window-controls = Window controls
settings-window-controls-detail = Draw minimise, maximise and close in the title bar
settings-controls-side = Controls side
settings-controls-side-detail = Which end of the title bar the controls sit on
settings-normalisation = Normalise loudness
settings-normalisation-detail = Keeps tracks at a consistent volume
settings-account = Account
settings-account-detail = Sign out of Spotify on this device
settings-sign-out = Sign out
settings-tab-about = About
settings-version = Version
settings-version-detail = The build of sonora you are running
settings-license = License
settings-license-detail = GNU General Public License version 3 or later
settings-license-view = Read the license
settings-source = Source code
settings-source-detail = The corresponding source for this build
settings-source-view = Open the repository
settings-team = Team
settings-team-github = GitHub
settings-role-lead-maintainer = Lead Maintainer
settings-role-maintainer = Maintainer
settings-role-contributor = Contributor
settings-notice = Copyright © 2026 nolight132. Sonora comes with absolutely no warranty. It is free software, and you are welcome to redistribute it under the terms of the GNU General Public License version 3 or later. Sonora is unofficial and is not affiliated with Spotify AB.

# themes
theme-dark = Dark
theme-light = Light
theme-midnight = Midnight
theme-forest = Forest
theme-ocean = Ocean
theme-rose = Rose
theme-lavender = Lavender
theme-amber = Amber

# corners
corners-square = Square
corners-subtle = Subtle
corners-rounded = Rounded
corners-round = Round

toast-playlist-created = Playlist created
toast-playlist-renamed = Playlist renamed
toast-playlist-deleted = Playlist deleted
toast-playlist-removed = Playlist removed from your library
toast-playlist-visibility = Playlist visibility changed
toast-track-added = Added to { $name }
toast-playlist-failed = That change could not be saved
toast-playlist-busy = Another change is still running
toast-playlist-signed-out = Sign in to change playlists
toast-queued-track = { $name } added to the queue
toast-next-track = { $name } plays next
toast-queued-album = Album added to the queue
toast-next-album = Album plays next
toast-queued-playlist = Playlist added to the queue
toast-next-playlist = Playlist plays next
toast-queue-failed = That could not be added to the queue
