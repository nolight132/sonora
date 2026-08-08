// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{Context, Entity, Task};
use spotify::{Album, Playlist, SpotifyApi, Track};

use crate::{Io, Session, SessionEvent, join};

const PAGE_LIMIT: u32 = 10000;

type Mutation = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;

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

pub struct Library {
    state: LibraryState,
    session: Entity<Session>,
    io: Io,
    task: Option<Task<()>>,
    playlist_task: Option<Task<()>>,
    pending: HashMap<String, Task<()>>,
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

    pub fn create_playlist(&mut self, name: String, cx: &mut Context<Self>) {
        self.mutate_playlist("create playlist", cx, move |client| {
            Box::pin(async move { client.create_playlist(&name).await })
        });
    }

    pub fn rename_playlist(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        self.mutate_playlist("rename playlist", cx, move |client| {
            Box::pin(async move { client.rename_playlist(&id, &name).await })
        });
    }

    pub fn set_playlist_public(&mut self, id: String, public: bool, cx: &mut Context<Self>) {
        self.mutate_playlist("change playlist visibility", cx, move |client| {
            Box::pin(async move { client.set_playlist_public(&id, public).await })
        });
    }

    pub fn add_to_playlist(
        &mut self,
        playlist_id: String,
        track_id: String,
        cx: &mut Context<Self>,
    ) {
        self.mutate_playlist("add track to playlist", cx, move |client| {
            Box::pin(async move { client.add_track_to_playlist(&playlist_id, &track_id).await })
        });
    }

    pub fn delete_playlist(&mut self, id: String, cx: &mut Context<Self>) {
        self.mutate_playlist("delete playlist", cx, move |client| {
            Box::pin(async move { client.delete_playlist(&id).await })
        });
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

    fn mutate_playlist<F>(&mut self, action: &'static str, cx: &mut Context<Self>, mutation: F)
    where
        F: FnOnce(Arc<dyn SpotifyApi>) -> Mutation + Send + 'static,
    {
        if self.playlist_task.is_some() {
            return;
        }
        let Some(client) = self.session.read(cx).client() else {
            return;
        };
        let io = self.io.clone();
        self.playlist_task = Some(cx.spawn(async move |this, cx| {
            let result = join(io.spawn({
                let request = client.clone();
                async move { mutation(request).await }
            }))
            .await;
            this.update(cx, |this, cx| match result {
                Ok(()) => this.load(client, cx),
                Err(error) => {
                    this.playlist_task = None;
                    log::warn!("library: cannot {action}: {error:#}");
                    cx.notify();
                }
            })
            .ok();
        }));
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
