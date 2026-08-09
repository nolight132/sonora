// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use anyhow::Result;
use librespot_core::Session;
use serde::Deserialize;

use super::query;

const HASH: &str = "ae0e2958a4ab645b35ca19ac04d0495ae12d9c5d7b7286217674801a9aab281a";

#[derive(Deserialize)]
struct Data {
    #[serde(rename = "artistUnion")]
    artist: Option<Artist>,
}

#[derive(Deserialize)]
struct Artist {
    discography: Option<Discography>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Discography {
    top_tracks: Option<Tracks>,
}

#[derive(Deserialize)]
struct Tracks {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    track: Track,
}

#[derive(Deserialize)]
struct Track {
    uri: String,
    playcount: Option<String>,
}

pub(crate) async fn artist(session: &Session, artist_id: &str) -> Result<HashMap<String, u64>> {
    let variables = variables(artist_id);
    let data = query::<Data>(session, "queryArtistOverview", HASH, variables).await?;
    Ok(playcounts(data))
}

fn variables(artist_id: &str) -> serde_json::Value {
    serde_json::json!({
        "uri": format!("spotify:artist:{artist_id}"),
        "locale": "",
        "preReleaseV2": true,
    })
}

fn playcounts(data: Data) -> HashMap<String, u64> {
    data.artist
        .and_then(|artist| artist.discography)
        .and_then(|discography| discography.top_tracks)
        .into_iter()
        .flat_map(|tracks| tracks.items)
        .filter_map(|item| {
            let count = item.track.playcount?.parse().ok()?;
            Some((item.track.uri, count))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_playcounts() {
        let data: Data = serde_json::from_slice(
            br#"{"artistUnion":{"discography":{"topTracks":{"items":[{"track":{"uri":"spotify:track:abc","playcount":"57545277"}},{"track":{"uri":"spotify:track:def","playcount":null}}]}}}}"#,
        )
        .unwrap();
        let counts = playcounts(data);

        assert_eq!(counts.get("spotify:track:abc"), Some(&57_545_277));
        assert!(!counts.contains_key("spotify:track:def"));
    }

    #[test]
    fn sends_current_overview_variables() {
        assert_eq!(
            variables("artist1"),
            serde_json::json!({
                "uri": "spotify:artist:artist1",
                "locale": "",
                "preReleaseV2": true,
            })
        );
    }
}
