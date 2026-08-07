// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use anyhow::{Context as _, Result};
use librespot_core::{Session, SpotifyUri};
use librespot_protocol::extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery};
use librespot_protocol::extension_kind::ExtensionKind;
use librespot_protocol::metadata::image::Size as ImageSize;
use librespot_protocol::metadata::{Artist as ArtistMessage, Image};
use protobuf::{EnumOrUnknown, Message as _};

use crate::models::{Album, Artist, Track};
use crate::{albums, collection, wire};

const ARTIST_PREFIX: &str = "spotify:artist:";
const ALBUM_PREFIX: &str = "spotify:album:";
const TRACK_PREFIX: &str = "spotify:track:";
const LARGE_PORTRAIT: i32 = 300;

pub async fn artist(session: &Session, artist_id: &str) -> Result<Artist> {
    let uri = SpotifyUri::from_uri(&format!("{ARTIST_PREFIX}{artist_id}"))
        .context("invalid artist ID")?;
    let body = session
        .spclient()
        .get_artist_metadata(&uri)
        .await
        .context("cannot read artist metadata")?;
    let message =
        ArtistMessage::parse_from_bytes(&body).context("cannot decode artist metadata protobuf")?;

    let track_uris = top_track_uris(&message, &session.country());
    let release_uris = release_uris(&message);
    let tracks = async {
        match track_uris.is_empty() {
            true => Ok(HashMap::<String, Track>::new()),
            false => collection::metadata(session, &track_uris).await,
        }
    };
    let releases = async {
        match release_uris.is_empty() {
            true => Ok(HashMap::<String, Album>::new()),
            false => albums::metadata(session, &release_uris).await,
        }
    };
    let (mut known_tracks, mut known_albums) = tokio::try_join!(tracks, releases)?;
    let top_tracks = track_uris
        .iter()
        .filter_map(|uri| known_tracks.remove(uri))
        .collect();
    let releases = release_uris
        .iter()
        .filter_map(|uri| known_albums.remove(uri))
        .collect();

    Ok(artist_from(&message, top_tracks, releases))
}

pub async fn images(session: &Session, ids: &[String]) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let request = BatchedEntityRequest {
        entity_request: ids
            .iter()
            .map(|id| EntityRequest {
                entity_uri: format!("{ARTIST_PREFIX}{id}"),
                query: vec![ExtensionQuery {
                    extension_kind: EnumOrUnknown::new(ExtensionKind::ARTIST_V4),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    };

    let response = session
        .spclient()
        .get_extended_metadata(request)
        .await
        .context("cannot read artist portraits")?;

    let mut found = HashMap::new();
    for array in response.extended_metadata {
        for entity in array.extension_data {
            let Ok(message) = ArtistMessage::parse_from_bytes(&entity.extension_data.value) else {
                continue;
            };
            let Some(id) = entity.entity_uri.strip_prefix(ARTIST_PREFIX) else {
                continue;
            };
            let smallest = portraits(&message)
                .into_iter()
                .min_by_key(|image| image_width(image));
            if let Some(url) = smallest.and_then(|image| wire::image_url(image.file_id())) {
                found.insert(id.to_owned(), url);
            }
        }
    }

    Ok(found)
}

fn artist_from(artist: &ArtistMessage, top_tracks: Vec<Track>, albums: Vec<Album>) -> Artist {
    let portraits = portraits(artist);

    Artist {
        name: artist.name().to_owned(),
        cover_large: portraits
            .iter()
            .filter(|image| image_width(image) >= LARGE_PORTRAIT)
            .min_by_key(|image| image_width(image))
            .or_else(|| portraits.iter().max_by_key(|image| image_width(image)))
            .and_then(|image| wire::image_url(image.file_id())),
        biography: artist.biography.iter().find_map(|bio| {
            bio.text
                .as_deref()
                .filter(|text| !text.is_empty())
                .map(plain_text)
                .filter(|text| !text.is_empty())
        }),
        top_tracks,
        albums,
    }
}

fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut tag = String::new();
    let mut inside_tag = false;

    for character in html.chars() {
        match character {
            '<' if !inside_tag => {
                inside_tag = true;
                tag.clear();
            }
            '>' if inside_tag => {
                inside_tag = false;
                let tag = tag.trim().to_ascii_lowercase();
                if (tag.starts_with("br")
                    || tag.starts_with("/p")
                    || tag.starts_with("/li")
                    || tag.starts_with("/div"))
                    && !text.ends_with(char::is_whitespace)
                {
                    text.push(' ');
                }
            }
            _ if inside_tag => tag.push(character),
            _ => text.push(character),
        }
    }

    let decoded = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");

    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::plain_text;

    #[test]
    fn biography_html_becomes_readable_text() {
        assert_eq!(
            plain_text(
                "Formed by <a href=\"spotify:artist:abc\">Alice &amp; Bob</a>.<br>Based in Paris."
            ),
            "Formed by Alice & Bob. Based in Paris."
        );
    }
}

fn top_track_uris(artist: &ArtistMessage, country: &str) -> Vec<String> {
    artist
        .top_track
        .iter()
        .find(|tracks| tracks.country() == country)
        .or_else(|| {
            artist
                .top_track
                .iter()
                .find(|tracks| tracks.country().is_empty())
        })
        .into_iter()
        .flat_map(|tracks| tracks.track.iter())
        .filter_map(|track| collection::base62(track.gid()))
        .map(|id| format!("{TRACK_PREFIX}{id}"))
        .collect()
}

fn release_uris(artist: &ArtistMessage) -> Vec<String> {
    let mut seen = HashSet::new();

    artist
        .album_group
        .iter()
        .chain(artist.single_group.iter())
        .filter_map(|group| group.album.first())
        .filter_map(|album| collection::base62(album.gid()))
        .filter(|id| seen.insert(id.clone()))
        .map(|id| format!("{ALBUM_PREFIX}{id}"))
        .collect()
}

fn portraits(artist: &ArtistMessage) -> Vec<&Image> {
    let mut portraits: Vec<_> = artist
        .portrait_group
        .as_ref()
        .into_iter()
        .flat_map(|group| group.image.iter())
        .filter(|image| image.has_file_id())
        .collect();
    portraits.extend(artist.portrait.iter().filter(|image| image.has_file_id()));
    portraits
}

fn image_width(image: &Image) -> i32 {
    if image.width() > 0 {
        return image.width();
    }

    match image.size() {
        ImageSize::SMALL => 64,
        ImageSize::DEFAULT => 300,
        ImageSize::LARGE => 640,
        ImageSize::XLARGE => 1_000,
    }
}
