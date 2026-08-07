// SPDX-License-Identifier: GPL-3.0-or-later

pub mod auth;

mod albums;
mod artists;
mod client;
mod collection;
mod collection2;
mod models;
mod pathfinder;
mod pb;
mod playlists;
mod profiles;
mod radio;
mod search;
mod wire;

pub use auth::AuthConfig;
pub use client::{LibrespotClient, SpotifyApi};
pub use models::{
    Album, AlbumDetail, Artist, ArtistRef, Credit, Playlist, ReleaseType, Track, UserProfile,
};
