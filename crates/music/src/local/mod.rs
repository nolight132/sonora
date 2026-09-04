mod client;
mod playback;
mod scan;
mod store;
mod tags;
mod wire;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{
    InputSource, MusicApi, MusicProvider, PlaybackFactory, PromptSink, ProviderSession, SignIn,
    UserProfile,
};

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    path: Option<PathBuf>,
}

pub struct LocalProvider {
    state_dir: PathBuf,
}

impl LocalProvider {
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    fn sidecar_path(&self) -> PathBuf {
        self.state_dir.join("local-music.json")
    }

    fn read_stored(&self) -> Stored {
        std::fs::read(self.sidecar_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write_stored(&self, stored: &Stored) -> Result<()> {
        std::fs::create_dir_all(&self.state_dir)
            .with_context(|| format!("cannot create {}", self.state_dir.display()))?;
        let bytes =
            serde_json::to_vec_pretty(stored).context("cannot serialize local music config")?;
        std::fs::write(self.sidecar_path(), bytes)
            .with_context(|| format!("cannot write {}", self.sidecar_path().display()))
    }

    async fn scan_path(&self, path: PathBuf) -> Result<ProviderSession> {
        let cache_dir = self.state_dir.clone();
        let scanned = tokio::task::spawn_blocking(move || scan::scan(&path, &cache_dir))
            .await
            .context("local scan task panicked")?;

        let api: Arc<dyn MusicApi> = Arc::new(client::LocalClient::new(scanned, &self.state_dir));
        let playback: Arc<dyn PlaybackFactory> = Arc::new(playback::Factory);

        Ok(ProviderSession {
            profile: UserProfile {
                id: "local".to_owned(),
                display_name: "Local Files".to_owned(),
            },
            api,
            playback,
            remotes: None,
            authenticated: false,
            playcounts: false,
        })
    }
}

#[async_trait]
impl MusicProvider for LocalProvider {
    fn name(&self) -> &'static str {
        "Local Files"
    }

    fn slug(&self) -> &'static str {
        "local"
    }

    fn sign_in_options(&self) -> Vec<SignIn> {
        Vec::new()
    }

    fn stored(&self) -> bool {
        self.read_stored().path.is_some()
    }

    fn location(&self) -> Option<String> {
        self.read_stored()
            .path
            .map(|path| path.display().to_string())
    }

    async fn restore(&self) -> Result<Option<ProviderSession>> {
        let Some(path) = self.read_stored().path else {
            return Ok(None);
        };
        self.scan_path(path).await.map(Some)
    }

    async fn sign_in(
        &self,
        method: SignIn,
        _prompt: PromptSink,
        _input: InputSource,
    ) -> Result<ProviderSession> {
        let SignIn::Path(path) = method else {
            return Err(anyhow!(
                "local files can only be configured with a folder path"
            ));
        };
        self.write_stored(&Stored {
            path: Some(path.clone()),
        })?;
        self.scan_path(path).await
    }

    fn sign_out(&self) {
        let _ = std::fs::remove_file(self.sidecar_path());
    }
}
