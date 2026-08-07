// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::collections::{HashMap, HashSet};

use gpui::{Context, Entity, Task};
use spotify::{AlbumDetail, Artist, Track};
use tokio::task::AbortHandle;

use crate::{Io, Session, SessionEvent, join};

pub struct SongDetail {
    id: Option<String>,
    track: Option<Track>,
    album: Option<AlbumDetail>,
    artist: Option<Artist>,
    portraits: HashMap<String, String>,
    playcount: Option<u64>,
    loading: bool,
    error: Option<String>,
    session: Entity<Session>,
    io: Io,
    task: Option<Task<()>>,
    request: Option<AbortHandle>,
}

impl SongDetail {
    pub fn new(session: Entity<Session>, io: Io, cx: &mut Context<Self>) -> Self {
        cx.subscribe(&session, |this, _, event, cx| {
            if matches!(event, SessionEvent::SignedOut) {
                this.clear();
                cx.notify();
            }
        })
        .detach();
        Self {
            id: None,
            track: None,
            album: None,
            artist: None,
            portraits: HashMap::new(),
            playcount: None,
            loading: false,
            error: None,
            session,
            io,
            task: None,
            request: None,
        }
    }

    pub fn track(&self) -> Option<&Track> {
        self.track.as_ref()
    }
    pub fn album(&self) -> Option<&AlbumDetail> {
        self.album.as_ref()
    }
    pub fn artist(&self) -> Option<&Artist> {
        self.artist.as_ref()
    }
    pub fn portraits(&self) -> &HashMap<String, String> {
        &self.portraits
    }
    pub fn playcount(&self) -> Option<u64> {
        self.playcount
    }
    pub fn is_loading(&self) -> bool {
        self.loading
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.id.as_deref() == Some(id) && (self.loading || self.track.is_some()) {
            return;
        }
        self.clear();
        self.id = Some(id.to_owned());
        let Some(client) = self.session.read(cx).client() else {
            cx.notify();
            return;
        };
        self.loading = true;
        cx.notify();
        let id = id.to_owned();
        let request = self.io.spawn({
            let id = id.clone();
            async move {
                let track = client.track(&id).await?;
                let album_id = track.album_id.clone();
                let artist_id = track
                    .artist_refs
                    .first()
                    .and_then(|artist| artist.id.clone());
                let credit_ids = track
                    .credits
                    .iter()
                    .filter_map(|credit| credit.id.clone())
                    .chain(
                        track
                            .artist_refs
                            .iter()
                            .filter_map(|artist| artist.id.clone()),
                    )
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                let album = async {
                    match album_id {
                        Some(album_id) => client.album(&album_id).await.ok(),
                        None => None,
                    }
                };
                let artist = async {
                    match artist_id {
                        Some(artist_id) => client.artist(&artist_id).await.ok(),
                        None => None,
                    }
                };
                let portraits = async {
                    match credit_ids.is_empty() {
                        true => HashMap::new(),
                        false => client.artist_images(credit_ids).await.unwrap_or_default(),
                    }
                };
                let playcount = async {
                    match track.id.as_deref() {
                        Some(track_id) => match client.track_playcount(track_id).await {
                            Ok(playcount) => playcount,
                            Err(error) => {
                                log::warn!("song: cannot read track play count: {error:#}");
                                None
                            }
                        },
                        None => None,
                    }
                };
                let (album, artist, portraits, playcount) =
                    tokio::join!(album, artist, portraits, playcount);
                anyhow::Ok((track, album, artist, portraits, playcount))
            }
        });
        self.request = Some(request.abort_handle());
        self.task = Some(cx.spawn(async move |this, cx| {
            let loaded = join(request).await;
            this.update(cx, |this, cx| {
                if this.id.as_deref() != Some(id.as_str()) {
                    return;
                }
                this.loading = false;
                this.request = None;
                match loaded {
                    Ok((track, album, artist, portraits, playcount)) => {
                        this.track = Some(track);
                        this.album = album;
                        this.artist = artist;
                        this.portraits = portraits;
                        this.playcount = playcount;
                    }
                    Err(error) => this.error = Some(format!("{error:#}")),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn clear(&mut self) {
        self.task = None;
        if let Some(request) = self.request.take() {
            request.abort();
        }
        self.id = None;
        self.track = None;
        self.album = None;
        self.artist = None;
        self.portraits.clear();
        self.playcount = None;
        self.loading = false;
        self.error = None;
    }
}
