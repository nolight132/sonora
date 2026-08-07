// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use anyhow::{Context as _, Result};
use librespot_core::Session;
use librespot_protocol::extended_metadata::{BatchedEntityRequest, EntityRequest, ExtensionQuery};
use librespot_protocol::extension_kind::ExtensionKind;
use librespot_protocol::metadata::Album as AlbumMessage;
use librespot_protocol::metadata::album::Type as AlbumType;
use protobuf::{EnumOrUnknown, Message as _};

use crate::models::{Album, AlbumDetail, ReleaseType, Track};
use crate::{collection, collection2, pathfinder, wire};

const ALBUM_PREFIX: &str = "spotify:album:";
const TRACK_PREFIX: &str = "spotify:track:";
const UNKNOWN: &str = "Unknown";

pub async fn saved_albums(session: &Session, limit: u32) -> Result<Vec<Album>> {
    let uris = collection2::saved_uris(session, ALBUM_PREFIX, limit as usize).await?;
    if uris.is_empty() {
        return Ok(Vec::new());
    }

    let mut known = metadata(session, &uris).await?;
    Ok(uris.iter().filter_map(|uri| known.remove(uri)).collect())
}

pub async fn album(session: &Session, album_id: &str) -> Result<AlbumDetail> {
    match pathfinder::album(session, album_id).await {
        Ok(album) => return Ok(album),
        Err(error) => log::warn!("albums: cannot load Pathfinder album: {error:#}"),
    }

    legacy_album(session, album_id).await
}

async fn legacy_album(session: &Session, album_id: &str) -> Result<AlbumDetail> {
    let uri = format!("{ALBUM_PREFIX}{album_id}");
    let request = batched(std::slice::from_ref(&uri));
    let response = session
        .spclient()
        .get_extended_metadata(request)
        .await
        .context("cannot read album metadata")?;

    let message = response
        .extended_metadata
        .into_iter()
        .flat_map(|array| array.extension_data)
        .find_map(|entity| AlbumMessage::parse_from_bytes(&entity.extension_data.value).ok())
        .context("album metadata is missing")?;
    let album = album_from(&uri, &message);
    let uris: Vec<_> = track_ids(&message)
        .into_iter()
        .map(|id| format!("{TRACK_PREFIX}{id}"))
        .collect();
    let tracks = match uris.is_empty() {
        true => Vec::new(),
        false => {
            let mut known = collection::metadata(session, &uris).await?;
            uris.iter().filter_map(|uri| known.remove(uri)).collect()
        }
    };
    Ok(AlbumDetail { album, tracks })
}

pub async fn album_tracks(session: &Session, album_id: &str) -> Result<Vec<Track>> {
    Ok(album(session, album_id).await?.tracks)
}

fn track_ids(album: &AlbumMessage) -> Vec<String> {
    album
        .disc
        .iter()
        .flat_map(|disc| disc.track.iter())
        .filter_map(|track| {
            let gid = track.gid.as_ref()?;
            librespot_core::SpotifyId::from_raw(gid)
                .ok()?
                .to_base62()
                .ok()
        })
        .collect()
}

fn batched(uris: &[String]) -> BatchedEntityRequest {
    BatchedEntityRequest {
        entity_request: uris
            .iter()
            .map(|uri| EntityRequest {
                entity_uri: uri.clone(),
                query: vec![ExtensionQuery {
                    extension_kind: EnumOrUnknown::new(ExtensionKind::ALBUM_V4),
                    ..Default::default()
                }],
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

pub(crate) async fn metadata(session: &Session, uris: &[String]) -> Result<HashMap<String, Album>> {
    let request = batched(uris);

    let response = session
        .spclient()
        .get_extended_metadata(request)
        .await
        .context("cannot read album metadata")?;

    let mut albums = HashMap::new();
    for array in response.extended_metadata {
        for entity in array.extension_data {
            let Ok(message) = AlbumMessage::parse_from_bytes(&entity.extension_data.value) else {
                continue;
            };
            let album = album_from(&entity.entity_uri, &message);
            albums.insert(entity.entity_uri, album);
        }
    }
    Ok(albums)
}

fn album_from(uri: &str, album: &AlbumMessage) -> Album {
    let (artists, artist_refs) = collection::artists_from(&album.artist);

    Album {
        id: uri.strip_prefix(ALBUM_PREFIX).unwrap_or(uri).to_owned(),
        name: non_empty(album.name.as_deref())
            .unwrap_or(UNKNOWN)
            .to_owned(),
        artists,
        artist_refs,
        cover: cover(album),
        cover_large: cover_large(album),
        release_type: release_type(album.type_()),
        year: album.date.as_ref().map(|date| date.year()).unwrap_or(0),
        track_count: track_ids(album).len() as u32,
        release_date: album
            .date
            .as_ref()
            .map(|date| {
                let year = date.year();
                let month = date.month();
                let day = date.day();
                match (month > 0, day > 0) {
                    (true, true) => format!("{year:04}-{month:02}-{day:02}"),
                    (true, false) => format!("{year:04}-{month:02}"),
                    _ => format!("{year:04}"),
                }
            })
            .unwrap_or_default(),
        label: album.label().to_owned(),
        copyrights: album
            .copyright
            .iter()
            .filter_map(|copyright| non_empty(copyright.text.as_deref()).map(str::to_owned))
            .collect(),
    }
}

fn release_type(kind: AlbumType) -> ReleaseType {
    match kind {
        AlbumType::ALBUM => ReleaseType::Album,
        AlbumType::SINGLE => ReleaseType::Single,
        AlbumType::COMPILATION => ReleaseType::Compilation,
        AlbumType::EP => ReleaseType::Ep,
        AlbumType::AUDIOBOOK => ReleaseType::Audiobook,
        AlbumType::PODCAST => ReleaseType::Podcast,
    }
}

fn cover(album: &AlbumMessage) -> Option<String> {
    let smallest = album
        .cover_group
        .as_ref()?
        .image
        .iter()
        .filter(|image| image.has_file_id())
        .min_by_key(|image| image.width())?;

    wire::image_url(smallest.file_id())
}

fn cover_large(album: &AlbumMessage) -> Option<String> {
    const HEADER: i32 = 300;

    let images = album.cover_group.as_ref()?.image.iter();
    let usable: Vec<_> = images.filter(|image| image.has_file_id()).collect();
    let picked = usable
        .iter()
        .filter(|image| image.width() >= HEADER)
        .min_by_key(|image| image.width())
        .or_else(|| usable.iter().max_by_key(|image| image.width()))?;

    wire::image_url(picked.file_id())
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}
