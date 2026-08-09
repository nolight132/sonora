// SPDX-License-Identifier: GPL-3.0-or-later

use gpui::{Context, Entity, Task};
use spotify::{Album, Artist, Track};
use tokio::task::AbortHandle;

use crate::{Io, Session, SessionEvent, join};

pub struct ArtistDetail {
    id: Option<String>,
    artist: Option<Artist>,
    loading: bool,
    error: Option<String>,
    session: Entity<Session>,
    io: Io,
    task: Option<Task<()>>,
    request: Option<AbortHandle>,
}

impl ArtistDetail {
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
            artist: None,
            loading: false,
            error: None,
            session,
            io,
            task: None,
            request: None,
        }
    }

    pub fn artist(&self) -> Option<&Artist> {
        self.artist.as_ref()
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn tracks(&self) -> &[Track] {
        self.artist
            .as_ref()
            .map(|artist| artist.top_tracks.as_slice())
            .unwrap_or_default()
    }

    pub fn albums(&self) -> &[Album] {
        self.artist
            .as_ref()
            .map(|artist| artist.albums.as_slice())
            .unwrap_or_default()
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.id.as_deref() == Some(id) && (self.loading || self.artist.is_some()) {
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
            async move { client.artist(&id).await }
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
                    Ok(artist) => this.artist = Some(artist),
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
        self.artist = None;
        self.loading = false;
        self.error = None;
    }
}
