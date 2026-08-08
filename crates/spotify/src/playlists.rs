// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context as _, Result};
use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue, Method};
use librespot_core::{Session, SpotifyId};
use librespot_protocol::playlist4_external::SelectedListContent;
use protobuf::Message as _;

use crate::collection;
use crate::models::Track;

const TRACK_PREFIX: &str = "spotify:track:";
const CONTENT: &str = "application/json";

pub async fn create(session: &Session, name: &str) -> Result<()> {
    let body = serde_json::to_vec(&serde_json::json!({
        "ops": [{
            "kind": "UPDATE_LIST_ATTRIBUTES",
            "updateListAttributes": {
                "newAttributes": { "values": { "name": name } }
            }
        }]
    }))?;
    session
        .spclient()
        .request(
            &Method::POST,
            "/playlist/v2/playlist",
            Some(headers()),
            Some(&body),
        )
        .await
        .context("cannot create the playlist")?;
    Ok(())
}

pub async fn rename(session: &Session, playlist_id: &str, name: &str) -> Result<()> {
    update(session, playlist_id, serde_json::json!({ "name": name }))
        .await
        .context("cannot rename the playlist")
}

pub async fn delete(session: &Session, playlist_id: &str) -> Result<()> {
    update(
        session,
        playlist_id,
        serde_json::json!({ "deletedByOwner": true }),
    )
    .await
    .context("cannot delete the playlist")
}

pub async fn set_public(session: &Session, playlist_id: &str, public: bool) -> Result<()> {
    let body = serde_json::to_vec(&serde_json::json!({
        "deltas": [{
            "ops": [{
                "kind": "UPDATE_ITEM_ATTRIBUTES",
                "updateItemAttributes": {
                    "item": { "uri": format!("spotify:playlist:{playlist_id}") },
                    "newAttributes": { "values": { "public": public } }
                }
            }],
            "info": { "source": { "client": "WEBPLAYER" } }
        }]
    }))?;
    let endpoint = format!("/playlist/v2/user/{}/rootlist/changes", session.username());
    session
        .spclient()
        .request(&Method::POST, &endpoint, Some(headers()), Some(&body))
        .await
        .context("cannot change playlist visibility")?;
    Ok(())
}

pub async fn add_track(session: &Session, playlist_id: &str, track_id: &str) -> Result<()> {
    let body = serde_json::to_vec(&serde_json::json!({
        "deltas": [{
            "ops": [{
                "kind": "ADD",
                "add": {
                    "items": [{ "uri": format!("spotify:track:{track_id}") }],
                    "addLast": true
                }
            }],
            "info": { "source": { "client": "WEBPLAYER" } }
        }]
    }))?;
    changes(session, playlist_id, &body)
        .await
        .context("cannot add the track to the playlist")
}

pub async fn remove_track(session: &Session, playlist_id: &str, track_id: &str) -> Result<()> {
    let body = serde_json::to_vec(&serde_json::json!({
        "deltas": [{
            "ops": [{
                "kind": "REM",
                "rem": {
                    "items": [{ "uri": format!("spotify:track:{track_id}") }]
                }
            }],
            "info": { "source": { "client": "WEBPLAYER" } }
        }]
    }))?;
    changes(session, playlist_id, &body)
        .await
        .context("cannot remove the track from the playlist")
}

pub async fn playlist_tracks(session: &Session, playlist_id: &str) -> Result<Vec<Track>> {
    let id = SpotifyId::from_base62(playlist_id).context("cannot read the playlist id")?;
    let body = session
        .spclient()
        .get_playlist(&id)
        .await
        .context("cannot read the playlist")?;

    let playlist =
        SelectedListContent::parse_from_bytes(&body).context("cannot decode the playlist")?;
    let uris: Vec<String> = playlist
        .contents
        .items
        .iter()
        .map(|item| item.uri())
        .filter(|uri| uri.starts_with(TRACK_PREFIX))
        .map(str::to_owned)
        .collect();
    if uris.is_empty() {
        return Ok(Vec::new());
    }

    let mut known = collection::metadata(session, &uris).await?;
    Ok(uris.iter().filter_map(|uri| known.remove(uri)).collect())
}

async fn update(session: &Session, playlist_id: &str, values: serde_json::Value) -> Result<()> {
    let body = serde_json::to_vec(&serde_json::json!({
        "deltas": [{
            "ops": [{
                "kind": "UPDATE_LIST_ATTRIBUTES",
                "updateListAttributes": {
                    "newAttributes": { "values": values }
                }
            }],
            "info": { "source": { "client": "WEBPLAYER" } }
        }]
    }))?;
    changes(session, playlist_id, &body).await
}

async fn changes(session: &Session, playlist_id: &str, body: &[u8]) -> Result<()> {
    let endpoint = format!("/playlist/v2/playlist/{playlist_id}/changes");
    session
        .spclient()
        .request(&Method::POST, &endpoint, Some(headers()), Some(body))
        .await?;
    Ok(())
}

fn headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(CONTENT));
    headers.insert(ACCEPT, HeaderValue::from_static(CONTENT));
    headers
}
