// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::sync::Arc;

use anyhow::Error;
use gpui::{Context, EventEmitter, Task};
use spotify::{AuthConfig, LibrespotClient, SpotifyApi, UserProfile, auth};

use crate::{Io, join};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    SignedOut,
    Restoring,
    Authorizing,
    SignedIn(UserProfile),
    Failed(String),
}

pub enum SessionEvent {
    SignedIn,
    SignedOut,
}

pub struct Session {
    state: SessionState,
    client: Option<Arc<LibrespotClient>>,
    config: AuthConfig,
    io: Io,
    task: Option<Task<()>>,
}

impl EventEmitter<SessionEvent> for Session {}

impl Session {
    pub fn new(config: AuthConfig, io: Io) -> Self {
        Self {
            state: SessionState::SignedOut,
            client: None,
            config,
            io,
            task: None,
        }
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn client(&self) -> Option<Arc<dyn SpotifyApi>> {
        self.client.clone().map(|c| c as Arc<dyn SpotifyApi>)
    }

    pub fn librespot(&self) -> Option<librespot_core::Session> {
        self.client.as_ref().map(|c| c.session().clone())
    }

    pub fn is_pending(&self) -> bool {
        matches!(
            self.state,
            SessionState::Restoring | SessionState::Authorizing
        )
    }

    pub fn restore(&mut self, cx: &mut Context<Self>) {
        if self.is_pending() {
            return;
        }
        self.state = SessionState::Restoring;
        cx.notify();

        let config = self.config.clone();
        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let restored = join(io.spawn(async move {
                let Some(client) = auth::restore(&config).await? else {
                    return anyhow::Ok(None);
                };
                let client = LibrespotClient::new(client);
                let profile = client.profile().await?;
                anyhow::Ok(Some((client, profile)))
            }))
            .await;

            this.update(cx, |this, cx| match restored {
                Ok(Some((client, profile))) => this.signed_in(client, profile, cx),
                Ok(None) => {
                    this.state = SessionState::SignedOut;
                    cx.notify();
                    cx.emit(SessionEvent::SignedOut);
                }
                Err(error) => this.failed(&error, cx),
            })
            .ok();
        }));
    }

    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        if self.is_pending() {
            return;
        }
        self.state = SessionState::Authorizing;
        cx.notify();

        let config = self.config.clone();
        let io = self.io.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let authorized = join(io.spawn(async move {
                let client = LibrespotClient::new(auth::login(&config).await?);
                let profile = client.profile().await?;
                anyhow::Ok((client, profile))
            }))
            .await;

            this.update(cx, |this, cx| match authorized {
                Ok((client, profile)) => this.signed_in(client, profile, cx),
                Err(error) => this.failed(&error, cx),
            })
            .ok();
        }));
    }

    pub fn sign_out(&mut self, cx: &mut Context<Self>) {
        auth::forget(&self.config);
        self.task = None;
        self.client = None;
        self.state = SessionState::SignedOut;
        cx.notify();
        cx.emit(SessionEvent::SignedOut);
    }

    fn signed_in(&mut self, client: LibrespotClient, profile: UserProfile, cx: &mut Context<Self>) {
        self.client = Some(Arc::new(client));
        self.state = SessionState::SignedIn(profile);
        cx.notify();
        cx.emit(SessionEvent::SignedIn);
    }

    fn failed(&mut self, error: &Error, cx: &mut Context<Self>) {
        self.client = None;
        self.state = SessionState::Failed(format!("{error:#}"));
        cx.notify();
        cx.emit(SessionEvent::SignedOut);
    }
}
