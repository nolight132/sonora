// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

mod artist;
mod detail;
mod home;
mod library;
mod playback;
mod queue;
mod search;
mod session;
mod settings;
mod song;

pub use artist::ArtistDetail;
pub use detail::{Collection, Detail, Header};
pub use home::Home;
pub use library::{Library, LibraryState};
pub use playback::{Origin, Playback, PlaybackState, Repeat};
pub use queue::Queue;
pub use search::{AlbumHit, ArtistHit, Hit, Kind, Search};
pub use session::{Session, SessionEvent, SessionState};
pub use settings::AppSettings;
pub use song::SongDetail;

use std::future::Future;
use std::sync::Arc;

use anyhow::Result;
use gpui::{App, AppContext as _, Entity, Global};
use spotify::AuthConfig;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct Io(Arc<Runtime>);

impl Global for Io {}

impl Io {
    pub fn new() -> Result<Self> {
        Ok(Self(Arc::new(Runtime::new()?)))
    }

    pub fn global(cx: &App) -> Self {
        cx.global::<Self>().clone()
    }

    pub fn handle(&self) -> tokio::runtime::Handle {
        self.0.handle().clone()
    }

    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.0.spawn(future)
    }
}

pub(crate) async fn join<T>(handle: JoinHandle<Result<T>>) -> Result<T> {
    handle.await?
}

pub struct Sonora {
    pub session: Entity<Session>,
    pub library: Entity<Library>,
    pub playback: Entity<Playback>,
    pub queue: Entity<Queue>,
    pub settings: Entity<AppSettings>,
}

impl Global for Sonora {}

impl Sonora {
    pub fn global(cx: &App) -> &Self {
        cx.global()
    }
}

pub fn init(cx: &mut App, io: Io) {
    cx.set_global(io.clone());

    let settings = cx.new(|_| AppSettings::load());
    let session = cx.new(|_| Session::new(AuthConfig::from_env(), io.clone()));
    let library = cx.new(|cx| Library::new(session.clone(), io, cx));
    let queue = cx.new(|_| Queue::new());
    let playback = cx.new(|cx| Playback::new(session.clone(), queue.clone(), settings.clone(), cx));

    cx.set_global(Sonora {
        session,
        library,
        playback,
        queue,
        settings,
    });
}
