// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use librespot_core::Session;
use librespot_protocol::playlist4_external::SelectedListContent as RootList;
use protobuf::Message as _;

use crate::models::{Album, AlbumDetail, Artist, Playlist, Track, UserProfile};
use crate::{
    albums, artists, collection, collection2, pathfinder, playlists, profiles, radio, search, wire,
};

#[async_trait]
pub trait SpotifyApi: Send + Sync {
    async fn profile(&self) -> Result<UserProfile>;
    async fn artist(&self, artist_id: &str) -> Result<Artist>;
    async fn artist_images(&self, ids: Vec<String>) -> Result<HashMap<String, String>>;
    async fn saved_tracks(&self, limit: u32) -> Result<Vec<Track>>;
    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()>;
    async fn track(&self, track_id: &str) -> Result<Track>;
    async fn track_playcount(&self, track_id: &str) -> Result<Option<u64>>;
    async fn playlists(&self, limit: u32) -> Result<Vec<Playlist>>;
    async fn create_playlist(&self, name: &str) -> Result<()>;
    async fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<()>;
    async fn delete_playlist(&self, playlist_id: &str) -> Result<()>;
    async fn set_playlist_public(&self, playlist_id: &str, public: bool) -> Result<()>;
    async fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()>;
    async fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()>;
    async fn saved_albums(&self, limit: u32) -> Result<Vec<Album>>;
    async fn album(&self, album_id: &str) -> Result<AlbumDetail>;
    async fn album_tracks(&self, album_id: &str) -> Result<Vec<Track>>;
    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>>;
    async fn track_radio(&self, track_id: &str) -> Result<Vec<Track>>;
    async fn search(&self, query: &str) -> Result<Vec<Track>>;
}

pub struct LibrespotClient {
    session: Session,
}

impl LibrespotClient {
    pub fn new(session: Session) -> Self {
        Self { session }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }
}

#[async_trait]
impl SpotifyApi for LibrespotClient {
    async fn profile(&self) -> Result<UserProfile> {
        let username = self.session.username();
        let body = self
            .session
            .spclient()
            .get_user_profile(&username, None, None)
            .await?;

        let profile: wire::Named = serde_json::from_slice(&body).unwrap_or_default();
        Ok(UserProfile {
            display_name: profile.label().unwrap_or(&username).to_owned(),
            id: username,
        })
    }

    async fn artist(&self, artist_id: &str) -> Result<Artist> {
        artists::artist(&self.session, artist_id).await
    }

    async fn artist_images(&self, ids: Vec<String>) -> Result<HashMap<String, String>> {
        artists::images(&self.session, &ids).await
    }

    async fn saved_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        collection::saved_tracks(&self.session, limit).await
    }

    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()> {
        collection2::set_track_saved(&self.session, track_id, saved).await
    }

    async fn track(&self, track_id: &str) -> Result<Track> {
        collection::track(&self.session, track_id).await
    }

    async fn track_playcount(&self, track_id: &str) -> Result<Option<u64>> {
        pathfinder::track(&self.session, track_id).await
    }

    async fn saved_albums(&self, limit: u32) -> Result<Vec<Album>> {
        albums::saved_albums(&self.session, limit).await
    }

    async fn album(&self, album_id: &str) -> Result<AlbumDetail> {
        albums::album(&self.session, album_id).await
    }

    async fn album_tracks(&self, album_id: &str) -> Result<Vec<Track>> {
        albums::album_tracks(&self.session, album_id).await
    }

    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        playlists::playlist_tracks(&self.session, playlist_id).await
    }

    async fn track_radio(&self, track_id: &str) -> Result<Vec<Track>> {
        radio::track_radio(&self.session, track_id).await
    }

    async fn search(&self, query: &str) -> Result<Vec<Track>> {
        search::search(&self.session, query).await
    }

    async fn create_playlist(&self, name: &str) -> Result<()> {
        playlists::create(&self.session, name).await
    }

    async fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<()> {
        playlists::rename(&self.session, playlist_id, name).await
    }

    async fn delete_playlist(&self, playlist_id: &str) -> Result<()> {
        playlists::delete(&self.session, playlist_id).await
    }

    async fn set_playlist_public(&self, playlist_id: &str, public: bool) -> Result<()> {
        playlists::set_public(&self.session, playlist_id, public).await
    }

    async fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        playlists::add_track(&self.session, playlist_id, track_id).await
    }

    async fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()> {
        playlists::remove_track(&self.session, playlist_id, track_id).await
    }

    async fn playlists(&self, limit: u32) -> Result<Vec<Playlist>> {
        let body = self
            .session
            .spclient()
            .get_rootlist(0, Some(limit as usize))
            .await?;

        let rootlist =
            RootList::parse_from_bytes(&body).context("cannot decode the rootlist protobuf")?;
        let mut playlists = wire::playlists_from(&rootlist);

        let owners = playlists
            .iter()
            .map(|playlist| playlist.owner.clone())
            .filter(|owner| owner != wire::UNKNOWN)
            .collect();
        let names = profiles::display_names(&self.session, owners).await;

        for playlist in &mut playlists {
            playlist.owned = playlist.owner == self.session.username();
            if let Some(name) = names.get(&playlist.owner) {
                playlist.owner = name.clone();
            }
        }

        Ok(playlists)
    }
}
