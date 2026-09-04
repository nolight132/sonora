pub mod auth;

mod albums;
mod artists;
mod client;
mod collection;
mod collection2;
mod lyrics;
mod pathfinder;
mod pb;
mod playback;
mod playlists;
mod profiles;
mod radio;
mod remotes;
mod search;
mod sink;
mod wire;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::spotify::playback::Factory;
use crate::spotify::remotes::Watcher;
use crate::{
    MusicApi as _, MusicProvider, ProviderSession, Remote, RemoteFactory, SignInFailure,
    SignInProblem,
};

pub use auth::AuthConfig;
pub use client::LibrespotClient;

struct Remotes(librespot_core::Session);

#[async_trait]
impl RemoteFactory for Remotes {
    async fn watch(&self, playable: bool) -> Result<Remote> {
        let (watcher, updates, orders) = Watcher::start(self.0.clone(), playable).await?;
        Ok(Remote {
            transport: Arc::new(watcher),
            updates: Box::new(updates),
            commands: Box::new(orders),
        })
    }
}

pub struct SpotifyProvider {
    config: AuthConfig,
}

impl SpotifyProvider {
    pub fn new(config: AuthConfig) -> Self {
        Self { config }
    }

    pub fn from_env() -> Self {
        Self::new(AuthConfig::from_env())
    }

    fn drop_free(&self, error: anyhow::Error) -> anyhow::Error {
        if matches!(
            error.downcast_ref::<SignInFailure>(),
            Some(SignInFailure(SignInProblem::Premium))
        ) {
            auth::forget(&self.config);
        }
        error
    }

    async fn session(&self, client: LibrespotClient) -> Result<ProviderSession> {
        let profile = client.profile().await?;
        let playback = Arc::new(Factory::new(client.session().clone()));
        let remotes = Arc::new(Remotes(client.session().clone()));
        Ok(ProviderSession {
            profile,
            api: Arc::new(client),
            playback,
            remotes: Some(remotes),
            authenticated: true,
            playcounts: true,
        })
    }
}

#[async_trait]
impl MusicProvider for SpotifyProvider {
    fn name(&self) -> &'static str {
        "Spotify"
    }

    fn slug(&self) -> &'static str {
        "spotify"
    }

    fn sign_in_options(&self) -> Vec<crate::SignIn> {
        vec![crate::SignIn::Default]
    }

    fn abandon(&self) {
        auth::release(&self.config);
    }

    fn stored(&self) -> bool {
        self.config.cache_dir.join("credentials.json").exists()
    }

    async fn restore(&self) -> Result<Option<ProviderSession>> {
        let Some(session) = auth::restore(&self.config)
            .await
            .map_err(|error| self.drop_free(error))?
        else {
            return Ok(None);
        };
        self.session(LibrespotClient::new(session)).await.map(Some)
    }

    async fn sign_in(
        &self,
        _method: crate::SignIn,
        _prompt: crate::PromptSink,
        _input: crate::InputSource,
    ) -> Result<ProviderSession> {
        let session = auth::login(&self.config)
            .await
            .map_err(|error| self.drop_free(error))?;
        self.session(LibrespotClient::new(session)).await
    }

    fn sign_out(&self) {
        auth::forget(&self.config);
    }
}
