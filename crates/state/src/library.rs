// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{Context, Entity, Task};
use spotify::{Album, Playlist, SpotifyApi, Track};

use crate::{Io, Note, Session, SessionEvent, Toasts, join};

const PAGE_LIMIT: u32 = 10000;

type Loaded = (
    anyhow::Result<Vec<Track>>,
    anyhow::Result<Vec<Playlist>>,
    anyhow::Result<Vec<Album>>,
);

fn partial(loaded: Loaded) -> LibraryState {
    let (tracks, playlists, albums) = loaded;
    if let (Err(tracks), Err(playlists), Err(albums)) = (&tracks, &playlists, &albums) {
        return LibraryState::Failed(format!("{tracks:#}\n{playlists:#}\n{albums:#}"));
    }

    let mut problems = Vec::new();
    LibraryState::Ready {
        tracks: take("Songs", tracks, &mut problems),
        playlists: take("Playlists", playlists, &mut problems),
        albums: take("Albums", albums, &mut problems),
        problems,
    }
}

fn take<T>(label: &str, result: anyhow::Result<Vec<T>>, problems: &mut Vec<String>) -> Vec<T> {
    result.unwrap_or_else(|error| {
        problems.push(format!("{label}: {error:#}"));
        Vec::new()
    })
}

pub enum LibraryEvent {
    PlaylistGone(String),
}

pub enum LibraryState {
    Empty,
    Loading,
    Ready {
        tracks: Vec<Track>,
        playlists: Vec<Playlist>,
        albums: Vec<Album>,
        problems: Vec<String>,
    },
    Failed(String),
}

impl gpui::EventEmitter<LibraryEvent> for Library {}

pub struct Library {
    state: LibraryState,
    session: Entity<Session>,
    io: Io,
    task: Option<Task<()>>,
    playlist_task: Option<Task<()>>,
    pending: HashMap<String, Task<()>>,
    pending_albums: HashMap<String, Task<()>>,
}

impl Library {
    pub fn new(session: Entity<Session>, io: Io, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&session, |this, session, event, cx| match event {
            SessionEvent::SignedIn => {
                let client = session.read(cx).client();
                if let Some(client) = client {
                    this.load(client, cx);
                }
            }
            SessionEvent::SignedOut => {
                this.task = None;
                this.playlist_task = None;
                this.pending.clear();
                this.pending_albums.clear();
                this.state = LibraryState::Empty;
                cx.notify();
            }
        })
        .detach();

        Self {
            state: LibraryState::Loading,
            session,
            io,
            task: None,
            playlist_task: None,
            pending: HashMap::new(),
            pending_albums: HashMap::new(),
        }
    }

    pub fn state(&self) -> &LibraryState {
        &self.state
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.state, LibraryState::Loading)
    }

    pub fn saved(&self, track_id: &str) -> bool {
        let LibraryState::Ready { tracks, .. } = &self.state else {
            return false;
        };
        tracks
            .iter()
            .any(|track| track.id.as_deref() == Some(track_id))
    }

    pub fn pending(&self, track_id: &str) -> bool {
        self.pending.contains_key(track_id)
    }

    pub fn toggle(&mut self, mut track: Track, cx: &mut Context<Self>) {
        let Some(track_id) = track.id.clone() else {
            return;
        };
        if self.pending(&track_id) {
            return;
        }
        let Some(client) = self.session.read(cx).client() else {
            return;
        };
        let saved = !self.saved(&track_id);
        let previous = match &self.state {
            LibraryState::Ready { tracks, .. } => tracks
                .iter()
                .find(|track| track.id.as_deref() == Some(track_id.as_str()))
                .cloned(),
            _ => None,
        };
        if saved {
            track.added_at = Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            );
        }
        self.set_saved(track.clone(), saved);

        let io = self.io.clone();
        let request_id = track_id.clone();
        let pending_id = track_id.clone();
        let task = cx.spawn(async move |this, cx| {
            let result =
                join(io.spawn(async move { client.set_track_saved(&request_id, saved).await }))
                    .await;

            this.update(cx, |this, cx| {
                this.pending.remove(&pending_id);
                if let Err(error) = result {
                    match previous {
                        Some(previous) => this.set_saved(previous, true),
                        None => this.set_saved(track, false),
                    }
                    log::warn!("library: cannot update saved track: {error:#}");
                }
                cx.notify();
            })
            .ok();
        });
        self.pending.insert(track_id, task);
        cx.notify();
    }

    pub fn saved_album(&self, album_id: &str) -> bool {
        self.album(album_id).is_some()
    }

    pub fn pending_album(&self, album_id: &str) -> bool {
        self.pending_albums.contains_key(album_id)
    }

    pub fn toggle_album(&mut self, album: Album, cx: &mut Context<Self>) {
        let album_id = album.id.clone();
        if self.pending_album(&album_id) {
            return;
        }
        let Some(client) = self.session.read(cx).client() else {
            return;
        };
        let saved = !self.saved_album(&album_id);
        let previous = self.album(&album_id).cloned();
        self.set_album_saved(album.clone(), saved);

        let io = self.io.clone();
        let request_id = album_id.clone();
        let pending_id = album_id.clone();
        let task = cx.spawn(async move |this, cx| {
            let result =
                join(io.spawn(async move { client.set_album_saved(&request_id, saved).await }))
                    .await;

            this.update(cx, |this, cx| {
                this.pending_albums.remove(&pending_id);
                if let Err(error) = result {
                    match previous {
                        Some(previous) => this.set_album_saved(previous, true),
                        None => this.set_album_saved(album, false),
                    }
                    log::warn!("library: cannot update saved album: {error:#}");
                }
                cx.notify();
            })
            .ok();
        });
        self.pending_albums.insert(album_id, task);
        cx.notify();
    }

    fn set_album_saved(&mut self, album: Album, saved: bool) {
        let LibraryState::Ready { albums, .. } = &mut self.state else {
            return;
        };
        match saved {
            true if !albums.iter().any(|known| known.id == album.id) => albums.push(album),
            false => albums.retain(|known| known.id != album.id),
            _ => {}
        }
    }

    pub fn create_playlist(&mut self, name: String, track: Option<String>, cx: &mut Context<Self>) {
        self.mutate_playlist(
            "create playlist",
            "toast-playlist-created",
            None,
            move |client| async move {
                let id = client.create_playlist(&name).await?;
                if let Some(track) = track {
                    client.add_track_to_playlist(&id, &track).await?;
                }
                client.playlist(&id).await.map(|detail| detail.playlist)
            },
            Self::insert_playlist,
            cx,
        );
    }

    pub fn rename_playlist(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        let renamed = (id.clone(), name.clone());
        self.mutate_playlist(
            "rename playlist",
            "toast-playlist-renamed",
            None,
            move |client| async move { client.rename_playlist(&id, &name).await },
            move |this, _, cx| {
                let (id, name) = renamed;
                this.amend_playlist(&id, |playlist| playlist.name = name, cx);
            },
            cx,
        );
    }

    pub fn set_playlist_public(&mut self, id: String, public: bool, cx: &mut Context<Self>) {
        let changed = id.clone();
        self.mutate_playlist(
            "change playlist visibility",
            "toast-playlist-visibility",
            None,
            move |client| async move { client.set_playlist_public(&id, public).await },
            move |this, _, cx| {
                this.amend_playlist(&changed, |playlist| playlist.public = public, cx);
            },
            cx,
        );
    }

    pub fn add_to_playlist(
        &mut self,
        playlist_id: String,
        track_id: String,
        cx: &mut Context<Self>,
    ) {
        let added = playlist_id.clone();
        let name = self
            .playlist(&playlist_id)
            .map(|playlist| playlist.name.clone());
        self.mutate_playlist(
            "add track to playlist",
            "toast-track-added",
            name,
            move |client| async move { client.add_track_to_playlist(&playlist_id, &track_id).await },
            move |this, _, cx| {
                this.amend_playlist(&added, |playlist| playlist.track_count += 1, cx);
            },
            cx,
        );
    }

    pub fn delete_playlist(&mut self, id: String, cx: &mut Context<Self>) {
        let deleted = id.clone();
        self.mutate_playlist(
            "delete playlist",
            "toast-playlist-deleted",
            None,
            move |client| async move { client.delete_playlist(&id).await },
            move |this, _, cx| this.forget_playlist(&deleted, cx),
            cx,
        );
    }

    pub fn add_playlist_to_library(&mut self, playlist: Playlist, cx: &mut Context<Self>) {
        let id = playlist.id.clone();
        self.mutate_playlist(
            "add playlist to library",
            "toast-playlist-added",
            None,
            move |client| async move { client.add_playlist_to_library(&id).await },
            move |this, _, cx| this.insert_playlist(playlist, cx),
            cx,
        );
    }

    pub fn remove_playlist_from_library(&mut self, id: String, cx: &mut Context<Self>) {
        let removed = id.clone();
        self.mutate_playlist(
            "remove playlist from library",
            "toast-playlist-removed",
            None,
            move |client| async move { client.remove_playlist_from_library(&id).await },
            move |this, _, cx| this.forget_playlist(&removed, cx),
            cx,
        );
    }

    pub fn album(&self, id: &str) -> Option<&Album> {
        let LibraryState::Ready { albums, .. } = &self.state else {
            return None;
        };
        albums.iter().find(|album| album.id == id)
    }

    pub fn playlist(&self, id: &str) -> Option<&Playlist> {
        let LibraryState::Ready { playlists, .. } = &self.state else {
            return None;
        };
        playlists.iter().find(|playlist| playlist.id == id)
    }

    fn mutate_playlist<F, R, T, A>(
        &mut self,
        action: &'static str,
        done: &'static str,
        name: Option<String>,
        mutation: F,
        apply: A,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(Arc<dyn SpotifyApi>) -> R + Send + 'static,
        R: Future<Output = anyhow::Result<T>> + Send + 'static,
        T: Send + 'static,
        A: FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
    {
        if self.playlist_task.is_some() {
            log::warn!("library: cannot {action} while another change is running");
            Toasts::show(Note::Failed, "toast-playlist-busy", cx);
            return;
        }
        let Some(client) = self.session.read(cx).client() else {
            log::warn!("library: cannot {action} while signed out");
            Toasts::show(Note::Failed, "toast-playlist-signed-out", cx);
            return;
        };
        let io = self.io.clone();
        self.playlist_task = Some(cx.spawn(async move |this, cx| {
            let result = join(io.spawn(async move { mutation(client).await })).await;
            this.update(cx, |this, cx| {
                this.playlist_task = None;
                match result {
                    Ok(outcome) => {
                        apply(this, outcome, cx);
                        match name {
                            Some(name) => Toasts::about(Note::Done, done, name, cx),
                            None => Toasts::show(Note::Done, done, cx),
                        }
                    }
                    Err(error) => {
                        log::warn!("library: cannot {action}: {error:#}");
                        Toasts::show(Note::Failed, "toast-playlist-failed", cx);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn insert_playlist(&mut self, playlist: Playlist, cx: &mut Context<Self>) {
        let LibraryState::Ready { playlists, .. } = &mut self.state else {
            return;
        };
        playlists.retain(|known| known.id != playlist.id);
        playlists.push(playlist);
        cx.notify();
    }

    fn forget_playlist(&mut self, id: &str, cx: &mut Context<Self>) {
        cx.emit(LibraryEvent::PlaylistGone(id.to_owned()));
        self.drop_playlist(id, cx);
    }

    fn drop_playlist(&mut self, id: &str, cx: &mut Context<Self>) {
        let LibraryState::Ready { playlists, .. } = &mut self.state else {
            return;
        };
        playlists.retain(|playlist| playlist.id != id);
        cx.notify();
    }

    fn amend_playlist(
        &mut self,
        id: &str,
        amend: impl FnOnce(&mut Playlist),
        cx: &mut Context<Self>,
    ) {
        let LibraryState::Ready { playlists, .. } = &mut self.state else {
            return;
        };
        let Some(playlist) = playlists.iter_mut().find(|playlist| playlist.id == id) else {
            return;
        };
        amend(playlist);
        cx.notify();
    }

    fn set_saved(&mut self, track: Track, saved: bool) {
        let LibraryState::Ready { tracks, .. } = &mut self.state else {
            return;
        };
        let id = track.id.as_deref();
        match saved {
            true if !tracks.iter().any(|saved| saved.id.as_deref() == id) => tracks.push(track),
            false => tracks.retain(|saved| saved.id.as_deref() != id),
            _ => {}
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let client = self.session.read(cx).client();
        if let Some(client) = client {
            self.load(client, cx);
        }
    }

    fn load(&mut self, client: Arc<dyn SpotifyApi>, cx: &mut Context<Self>) {
        self.playlist_task = None;
        self.pending.clear();
        self.pending_albums.clear();
        self.state = LibraryState::Loading;
        cx.notify();

        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let loaded = join(io.spawn(async move {
                anyhow::Ok(tokio::join!(
                    client.saved_tracks(PAGE_LIMIT),
                    client.playlists(PAGE_LIMIT),
                    client.saved_albums(PAGE_LIMIT)
                ))
            }))
            .await;

            this.update(cx, |this, cx| {
                this.state = match loaded {
                    Ok(loaded) => partial(loaded),
                    Err(error) => LibraryState::Failed(format!("{error:#}")),
                };
                cx.notify();
            })
            .ok();
        }));
    }
}
