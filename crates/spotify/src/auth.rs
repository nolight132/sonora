// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::path::PathBuf;

use anyhow::{Context as _, Result, anyhow};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::{Session, SessionConfig};
use librespot_oauth::OAuthClientBuilder;

pub const DEFAULT_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8989/login";

pub const SCOPES: &[&str] = &[
    "playlist-read-collaborative",
    "playlist-read-private",
    "streaming",
    "user-follow-read",
    "user-library-read",
    "user-read-email",
    "user-read-playback-state",
    "user-read-private",
    "user-read-recently-played",
    "user-top-read",
];

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub client_id: String,
    pub redirect_uri: String,
    pub cache_dir: PathBuf,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            client_id: DEFAULT_CLIENT_ID.to_owned(),
            redirect_uri: DEFAULT_REDIRECT_URI.to_owned(),
            cache_dir: default_cache_dir(),
        }
    }
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        if let Ok(redirect_uri) = std::env::var("SONORA_REDIRECT_URI") {
            config.redirect_uri = redirect_uri;
        }
        config
    }
}

fn default_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("sonora")
}

pub async fn restore(config: &AuthConfig) -> Result<Option<Session>> {
    let session = session(config)?;
    let Some(credentials) = session.cache().and_then(|cache| cache.credentials()) else {
        return Ok(None);
    };

    session.connect(credentials, true).await?;
    Ok(Some(session))
}

pub async fn login(config: &AuthConfig) -> Result<Session> {
    let client_id = config.client_id.clone();
    let redirect_uri = config.redirect_uri.clone();

    let token = tokio::task::spawn_blocking(move || {
        OAuthClientBuilder::new(&client_id, &redirect_uri, SCOPES.to_vec())
            .open_in_browser()
            .build()?
            .get_access_token()
    })
    .await?
    .map_err(explain)?;

    let session = session(config)?;
    session
        .connect(Credentials::with_access_token(token.access_token), true)
        .await
        .map_err(denied)?;
    Ok(session)
}

fn denied(error: librespot_core::Error) -> anyhow::Error {
    anyhow::Error::new(error).context(
        "Spotify refused the session. librespot can only open a session with one of Spotify's \
         own client ids, not a developer-app client id",
    )
}

fn explain(error: librespot_oauth::OAuthError) -> anyhow::Error {
    let message = error.to_string();
    match callback_error(&message) {
        Some("invalid_scope") => anyhow!("Spotify rejected the requested scopes (invalid_scope)"),
        Some("access_denied") => anyhow!("Authorization was denied in the browser"),
        Some(code) => anyhow!("Spotify refused authorization ({code})"),
        None => anyhow::Error::new(error).context("browser authorization failed"),
    }
}

fn callback_error(message: &str) -> Option<&str> {
    let start = message.find("error=")? + "error=".len();
    let rest = &message[start..];
    Some(rest.split(['&', ' ']).next().unwrap_or(rest))
}

pub fn forget(config: &AuthConfig) {
    let _ = std::fs::remove_file(config.cache_dir.join("credentials.json"));
}

fn session(config: &AuthConfig) -> Result<Session> {
    let cache = Cache::new(Some(config.cache_dir.as_path()), None, None, None)
        .with_context(|| format!("cannot open cache at {}", config.cache_dir.display()))?;

    let session_config = SessionConfig {
        client_id: config.client_id.clone(),
        ..Default::default()
    };

    Ok(Session::new(session_config, Some(cache)))
}
