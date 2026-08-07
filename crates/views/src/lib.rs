// SPDX-License-Identifier: GPL-3.0-or-later

mod adaptive;
mod artist;
mod cells;
mod columns;
mod detail;
mod hero;
mod home;
mod library;
mod login;
mod page;
mod quick_picks;
mod release_card;
mod root;
mod search;
mod settings;
mod song;
mod tracks;

use adaptive::Adaptive;
use artist::ArtistView;
pub use columns::ColumnPicker;
use detail::DetailView;
use home::HomeView;
pub use library::LibraryView;
pub use login::LoginView;
pub use root::Root;
pub use settings::SettingsView;
use song::SongView;
