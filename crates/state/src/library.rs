// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::sync::Arc;

use gpui::{Context, Entity, Task};
use spotify::{Album, Playlist, SpotifyApi, Track};

use crate::{Io, Session, SessionEvent, join};

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
        }
    }

    pub fn state(&self) -> &LibraryState {
        &self.state
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.state, LibraryState::Loading)
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

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let client = self.session.read(cx).client();
        if let Some(client) = client {
            self.load(client, cx);
        }
    }

    fn load(&mut self, client: Arc<dyn SpotifyApi>, cx: &mut Context<Self>) {
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
