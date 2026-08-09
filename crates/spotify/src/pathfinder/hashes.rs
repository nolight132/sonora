// SPDX-License-Identifier: GPL-3.0-or-later

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result, bail};
use bytes::Bytes;
use http::{Method, Request, header};
use librespot_core::Session;
use serde::{Deserialize, Serialize};

const WORKER: &str = "https://billowing-resonance-da83.johnwatson.workers.dev/hash";
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const FILE: &str = "pathfinder.json";

pub(super) struct Hash {
    pub(super) value: String,
    pub(super) tried: bool,
}

#[derive(Clone, Deserialize, Serialize)]
struct Entry {
    hash: String,
    fetched: u64,
}

#[derive(Deserialize)]
struct Answer {
    hash: String,
}

pub(super) async fn resolve(session: &Session, operation: &str) -> Result<Hash> {
    let Some(entry) = cached(operation) else {
        return Ok(Hash {
            value: fetch(session, operation)
                .await
                .inspect(|hash| store(operation, hash))?,
            tried: true,
        });
    };
    match aged(&entry) < MAX_AGE {
        true => Ok(Hash {
            value: entry.hash,
            tried: false,
        }),
        false => Ok(Hash {
            value: refresh(session, operation).await.unwrap_or(entry.hash),
            tried: true,
        }),
    }
}

pub(super) async fn refresh(session: &Session, operation: &str) -> Option<String> {
    fetch(session, operation)
        .await
        .inspect(|hash| store(operation, hash))
        .inspect_err(|error| {
            log::warn!("pathfinder: cannot refresh the {operation} hash: {error:#}")
        })
        .ok()
}

async fn fetch(session: &Session, operation: &str) -> Result<String> {
    let request = Request::builder()
        .method(Method::GET)
        .uri(format!("{WORKER}/{operation}"))
        .header(header::ACCEPT, "application/json")
        .body(Bytes::new())
        .with_context(|| format!("cannot build the {operation} hash request"))?;
    let body = session
        .http_client()
        .request_body(request)
        .await
        .with_context(|| format!("cannot request the {operation} hash"))?;
    let answer: Answer = serde_json::from_slice(&body)
        .with_context(|| format!("cannot decode the {operation} hash response"))?;
    if !sane(&answer.hash) {
        bail!("received a malformed {operation} hash");
    }
    Ok(answer.hash)
}

fn sane(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn aged(entry: &Entry) -> Duration {
    Duration::from_secs(now().saturating_sub(entry.fetched))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cached(operation: &str) -> Option<Entry> {
    entries().lock().ok()?.get(operation).cloned()
}

fn store(operation: &str, hash: &str) {
    let Ok(mut entries) = entries().lock() else {
        return;
    };
    entries.insert(
        operation.to_owned(),
        Entry {
            hash: hash.to_owned(),
            fetched: now(),
        },
    );
    write(&entries);
}

fn entries() -> &'static Mutex<HashMap<String, Entry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(read()))
}

fn read() -> HashMap<String, Entry> {
    let Ok(body) = std::fs::read(path()) else {
        return HashMap::new();
    };
    serde_json::from_slice(&body).unwrap_or_default()
}

fn write(entries: &HashMap<String, Entry>) {
    let path = path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let written = serde_json::to_vec_pretty(entries)
        .context("cannot encode")
        .and_then(|body| std::fs::write(&path, body).context("cannot save"));
    if let Err(error) = written {
        log::warn!("pathfinder: {error:#} {}", path.display());
    }
}

fn path() -> PathBuf {
    crate::auth::default_cache_dir().join(FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_persisted_query_hash() {
        assert!(sane(&"0123456789abcdef".repeat(4)));
    }

    #[test]
    fn rejects_a_malformed_hash() {
        assert!(!sane(""));
        assert!(!sane("0123456789abcdef"));
        assert!(!sane(&"z".repeat(64)));
    }

    #[test]
    fn ages_from_the_fetch_time() {
        let entry = Entry {
            hash: String::new(),
            fetched: now() - 60,
        };
        assert!(aged(&entry) >= Duration::from_secs(60));
        assert!(aged(&entry) < MAX_AGE);
    }
}
