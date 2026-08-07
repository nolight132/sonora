// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use gpui::{Context, Entity, Task};
use i18n::t;
use spotify::{Album, AlbumDetail, ArtistRef, Playlist, Track};

use crate::{Io, Library, Session, SessionEvent, join};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Collection {
    Album,
    Playlist,
}

enum Loaded {
    Album(AlbumDetail),
    Tracks(Vec<Track>),
}

pub struct Header {
    pub kind: Collection,
    pub title: String,
    pub artist: Option<String>,
    pub artist_refs: Vec<ArtistRef>,
    pub release_date: Option<String>,
    pub meta: Vec<String>,
    pub cover: Option<String>,
}

pub struct Detail {
    id: Option<String>,
    header: Option<Header>,
    tracks: Vec<Track>,
    loading: bool,
    error: Option<String>,
    session: Entity<Session>,
    library: Entity<Library>,
    io: Io,
    task: Option<Task<()>>,
}

impl Detail {
    pub fn new(
        session: Entity<Session>,
        library: Entity<Library>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.subscribe(&session, |this, _, event, cx| {
            if matches!(event, SessionEvent::SignedOut) {
                this.clear();
                cx.notify();
            }
        })
        .detach();

        Self {
            id: None,
            header: None,
            tracks: Vec::new(),
            loading: false,
            error: None,
            session,
            library,
            io,
            task: None,
        }
    }

    pub fn header(&self) -> Option<&Header> {
        self.header.as_ref()
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn open_album(&mut self, id: &str, cx: &mut Context<Self>) {
        let known = self.library.read(cx).album(id).map(album_header);
        self.open(Collection::Album, id, known, cx);
    }

    pub fn open_playlist(&mut self, id: &str, cx: &mut Context<Self>) {
        let known = self.library.read(cx).playlist(id).map(playlist_header);
        self.open(Collection::Playlist, id, known, cx);
    }

    fn open(&mut self, kind: Collection, id: &str, known: Option<Header>, cx: &mut Context<Self>) {
        if self.shows(id) {
            return;
        }

        self.clear();
        self.id = Some(id.to_owned());
        self.header = known;

        let Some(client) = self.session.read(cx).client() else {
            cx.notify();
            return;
        };

        self.loading = true;
        cx.notify();

        let io = self.io.clone();
        let id = id.to_owned();
        self.task = Some(cx.spawn(async move |this, cx| {
            let loaded = join(io.spawn(async move {
                match kind {
                    Collection::Album => client.album(&id).await.map(Loaded::Album),
                    Collection::Playlist => client.playlist_tracks(&id).await.map(Loaded::Tracks),
                }
            }))
            .await;

            this.update(cx, |this, cx| {
                this.loading = false;
                match loaded {
                    Ok(Loaded::Album(detail)) => {
                        this.header = Some(album_header(&detail.album));
                        this.tracks = detail.tracks;
                    }
                    Ok(Loaded::Tracks(tracks)) => this.tracks = tracks,
                    Err(error) => this.error = Some(format!("{error:#}")),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn shows(&self, id: &str) -> bool {
        let same = self.id.as_deref() == Some(id);
        same && (self.loading || !self.tracks.is_empty())
    }

    fn clear(&mut self) {
        self.task = None;
        self.id = None;
        self.header = None;
        self.tracks.clear();
        self.loading = false;
        self.error = None;
    }
}

fn album_header(album: &Album) -> Header {
    let mut parts = Vec::new();
    if album.track_count > 0 {
        parts.push(t!("count-songs", count = album.track_count).to_string());
    }

    Header {
        kind: Collection::Album,
        title: album.name.clone(),
        artist: Some(album.artists.clone()),
        artist_refs: album.artist_refs.clone(),
        release_date: match album.release_date.is_empty() {
            true => (album.year > 0).then(|| album.year.to_string()),
            false => Some(album.release_date.clone()),
        },
        meta: parts,
        cover: album.cover_large.clone(),
    }
}

fn playlist_header(playlist: &Playlist) -> Header {
    let mut parts = vec![playlist.owner.clone()];
    if playlist.track_count > 0 {
        parts.push(t!("count-songs", count = playlist.track_count).to_string());
    }

    Header {
        kind: Collection::Playlist,
        title: playlist.name.clone(),
        artist: None,
        artist_refs: Vec::new(),
        release_date: None,
        meta: parts,
        cover: playlist.cover.clone(),
    }
}
