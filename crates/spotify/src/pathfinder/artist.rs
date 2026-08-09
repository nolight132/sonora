// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use anyhow::Result;
use librespot_core::Session;
use serde::Deserialize;

use super::query;

const HASH: &str = "ae0e2958a4ab645b35ca19ac04d0495ae12d9c5d7b7286217674801a9aab281a";

#[derive(Default)]
pub(crate) struct Overview {
    pub(crate) playcounts: HashMap<String, u64>,
    pub(crate) monthly_listeners: Option<u64>,
}

#[derive(Deserialize)]
struct Data {
    #[serde(rename = "artistUnion")]
    artist: Option<Artist>,
}

#[derive(Deserialize)]
struct Artist {
    discography: Option<Discography>,
    stats: Option<Stats>,
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

#[derive(Deserialize)]
struct Stats {
    #[serde(rename = "monthlyListeners")]
    monthly_listeners: Option<u64>,
}

pub(crate) async fn artist(session: &Session, artist_id: &str) -> Result<Overview> {
    let variables = variables(artist_id);
    let data = query::<Data>(session, "queryArtistOverview", HASH, variables).await?;
    Ok(overview(data))
}

fn variables(artist_id: &str) -> serde_json::Value {
    serde_json::json!({
        "uri": format!("spotify:artist:{artist_id}"),
        "locale": "",
        "preReleaseV2": true,
    })
}

fn overview(data: Data) -> Overview {
    let Some(artist) = data.artist else {
        return Overview::default();
    };
    let playcounts = artist
        .discography
        .and_then(|discography| discography.top_tracks)
        .into_iter()
        .flat_map(|tracks| tracks.items)
        .filter_map(|item| {
            let count = item.track.playcount?.parse().ok()?;
            Some((item.track.uri, count))
        })
        .collect();
    Overview {
        playcounts,
        monthly_listeners: artist.stats.and_then(|stats| stats.monthly_listeners),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_overview() {
        let data: Data = serde_json::from_slice(
            br#"{"artistUnion":{"discography":{"topTracks":{"items":[{"track":{"uri":"spotify:track:abc","playcount":"57545277"}},{"track":{"uri":"spotify:track:def","playcount":null}}]}},"stats":{"monthlyListeners":1900430}}}"#,
        )
        .unwrap();
        let overview = overview(data);

        assert_eq!(overview.monthly_listeners, Some(1_900_430));
        assert_eq!(
            overview.playcounts.get("spotify:track:abc"),
            Some(&57_545_277)
        );
        assert!(!overview.playcounts.contains_key("spotify:track:def"));
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
