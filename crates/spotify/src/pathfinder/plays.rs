// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context as _, Result, anyhow};
use librespot_core::Session;
use serde::Deserialize;

use super::query;

const HASH: &str = "612585ae06ba435ad26369870deaae23b5c8800a256cd8a57e08eddc25a37294";

#[derive(Deserialize)]
struct Data {
    #[serde(rename = "trackUnion")]
    track: Option<Track>,
}

#[derive(Deserialize)]
struct Track {
    playcount: Option<String>,
}

pub(crate) async fn track(session: &Session, track_id: &str) -> Result<Option<u64>> {
    let variables = serde_json::json!({ "uri": format!("spotify:track:{track_id}") });
    let data = query::<Data>(session, "getTrack", HASH, variables).await?;
    playcount(data)
}

fn playcount(data: Data) -> Result<Option<u64>> {
    let Some(track) = data.track else {
        return Err(anyhow!("track play count response has no track"));
    };
    track
        .playcount
        .map(|count| count.parse().context("invalid track play count"))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_playcount() {
        let data: Data =
            serde_json::from_slice(br#"{"trackUnion":{"playcount":"1234567"}}"#).unwrap();
        assert_eq!(playcount(data).unwrap(), Some(1_234_567));
    }
}
