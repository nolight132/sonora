// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use std::future::Future;
use std::io::Read as _;
use std::pin::Pin;

use anyhow::{Result, anyhow};
use gpui::http_client::http::HeaderValue;
use gpui::http_client::{AsyncBody, HttpClient, Inner, Request, Response, Url};
use tokio::runtime::Handle;

const USER_AGENT: &str = "sonora";

type Sent = Pin<Box<dyn Future<Output = Result<Response<AsyncBody>>> + Send>>;

pub struct Client {
    inner: reqwest::Client,
    handle: Handle,
    user_agent: HeaderValue,
}

impl Client {
    pub fn new(handle: Handle) -> Self {
        let _guard = handle.enter();
        Self {
            inner: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .unwrap_or_default(),
            handle: handle.clone(),
            user_agent: HeaderValue::from_static(USER_AGENT),
        }
    }
}

impl HttpClient for Client {
    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(&self, request: Request<AsyncBody>) -> Sent {
        let client = self.inner.clone();
        let handle = self.handle.clone();

        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = read(body)?;

            let fetch = handle.spawn(async move {
                let mut outgoing = client
                    .request(parts.method, parts.uri.to_string())
                    .headers(parts.headers);

                if let Some(body) = body {
                    outgoing = outgoing.body(body);
                }

                let incoming = outgoing.send().await?;
                let status = incoming.status();
                let bytes = incoming.bytes().await?.to_vec();
                Ok::<_, anyhow::Error>((status, bytes))
            });

            let (status, bytes) = fetch.await??;
            Ok(Response::builder()
                .status(status)
                .body(AsyncBody::from(bytes))?)
        })
    }
}

fn read(body: AsyncBody) -> Result<Option<Vec<u8>>> {
    match body.0 {
        Inner::Empty => Ok(None),
        Inner::Bytes(mut cursor) => {
            let mut bytes = Vec::new();
            cursor.read_to_end(&mut bytes)?;
            Ok(Some(bytes))
        }
        Inner::AsyncReader(_) => Err(anyhow!("streaming request bodies are not supported")),
    }
}
