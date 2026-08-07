// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context as _, Result};
use librespot_core::{Session, SpotifyId};
use librespot_protocol::playlist4_external::SelectedListContent;
use protobuf::Message as _;

use crate::collection;
use crate::models::Track;

const TRACK_PREFIX: &str = "spotify:track:";

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
