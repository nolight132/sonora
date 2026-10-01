//! What an artist or an album page fills in behind its own rows, from the views the
//! web player reads its own rails off: the appears-on view of an artist, and the
//! related-albums view of an album with more from the same artist and its neighbours
//! beside it. A view that comes back empty leaves its rail off the page rather than
//! failing it.

use std::collections::HashSet;

use anyhow::{Context as _, Result};

use crate::apple::client::AppleClient;
use crate::apple::wire;
use crate::escape;
use crate::{Album, AlbumCatalogue, ArtistCatalogue, SUGGESTIONS, SavedArtist};

/// How many similar artists lend their releases to a thin rail, and how many releases each
/// lends.
const SIMILAR_ARTISTS: usize = 6;
const SIMILAR_RELEASES: usize = 2;

/// The appears-on view of one artist, read in a single request once a library id has been
/// turned into the catalog's.
pub(crate) async fn artist_catalogue(
    client: &AppleClient,
    artist_id: &str,
) -> Result<ArtistCatalogue> {
    let artist_id = client.catalog_artist(artist_id).await?;
    let answered = client
        .get(
            &client.catalog(&format!("/artists/{}", escape::component(&artist_id))),
            &[("views", "appears-on-albums")],
        )
        .await?;
    let found = answered
        .pointer("/data/0")
        .context("apple music has no such artist")?;
    Ok(ArtistCatalogue {
        appears_on: wire::view(found, "appears-on-albums")
            .iter()
            .filter_map(wire::album)
            .take(SUGGESTIONS)
            .collect(),
        ..Default::default()
    })
}

/// The related-albums view of one album, without the album itself. A library album that is
/// not in the catalog has no related albums to read.
async fn related_albums(client: &AppleClient, album_id: &str) -> Result<Vec<Album>> {
    if AppleClient::is_mine(album_id) {
        return Ok(Vec::new());
    }
    let answered = client
        .get(
            &client.catalog(&format!("/albums/{}", escape::component(album_id))),
            &[("views", "related-albums")],
        )
        .await
        .context("cannot read the related albums")?;
    let found = answered
        .pointer("/data/0")
        .context("apple music has no such album")?;
    Ok(wire::view(found, "related-albums")
        .iter()
        .filter_map(wire::album)
        .filter(|album| album.id != album_id)
        .collect())
}

/// The artist's own releases without the album the page is already showing, with the
/// artists Apple lists as similar beside them, read in a single request.
async fn more_from_artist(
    client: &AppleClient,
    album_id: &str,
    artist_id: &str,
) -> Result<(Vec<Album>, Vec<SavedArtist>)> {
    let artist_id = &client.catalog_artist(artist_id).await?;
    let answered = client
        .get(
            &client.catalog(&format!("/artists/{}", escape::component(artist_id))),
            &[("views", "full-albums,singles,similar-artists")],
        )
        .await
        .context("cannot read more from this artist")?;
    let found = answered
        .pointer("/data/0")
        .context("apple music has no such artist")?;
    let more = wire::view(found, "full-albums")
        .iter()
        .chain(wire::view(found, "singles").iter())
        .filter_map(wire::album)
        .filter(|album| album.id != album_id)
        .collect();
    let similar = wire::view(found, "similar-artists")
        .iter()
        .filter_map(wire::similar_artist)
        .collect();
    Ok((more, similar))
}

/// A few releases by one similar artist, skipping the page's own album.
async fn releases_by(client: &AppleClient, album_id: &str, artist_id: &str) -> Result<Vec<Album>> {
    let answered = client
        .get(
            &client.catalog(&format!("/artists/{}", escape::component(artist_id))),
            &[("views", "full-albums,singles")],
        )
        .await
        .with_context(|| format!("cannot read releases by similar artist {artist_id}"))?;
    let found = answered
        .pointer("/data/0")
        .with_context(|| format!("apple music has no such artist {artist_id}"))?;
    Ok(wire::view(found, "full-albums")
        .iter()
        .chain(wire::view(found, "singles").iter())
        .filter_map(wire::album)
        .filter(|album| album.id != album_id)
        .take(SIMILAR_RELEASES)
        .collect())
}

/// A few releases each from the first similar artists: the cross-artist half of the rail
/// when the related view comes back thin. One artist failing only shortens the rail.
async fn similar_releases(client: &AppleClient, album_id: &str, similar: &[String]) -> Vec<Album> {
    let reads = similar
        .iter()
        .take(SIMILAR_ARTISTS)
        .map(|artist_id| releases_by(client, album_id, artist_id));
    let mut releases = Vec::new();
    for read in futures::future::join_all(reads).await {
        match read {
            Ok(read) => releases.extend(read),
            Err(error) => log::warn!("apple: {error:#}"),
        }
    }
    releases
}

/// The related-albums view of one album with the artist's own releases first, topped up
/// from similar artists while the rail is thin, read together and deduplicated by id.
/// Either half failing only empties its own half; the page keeps the other.
pub(crate) async fn album_catalogue(
    client: &AppleClient,
    album_id: &str,
    artist_id: Option<&str>,
) -> Result<AlbumCatalogue> {
    let related = related_albums(client, album_id);
    let more = async {
        match artist_id {
            Some(artist_id) => more_from_artist(client, album_id, artist_id).await,
            None => Ok((Vec::new(), Vec::new())),
        }
    };
    let (also_like, more) = tokio::join!(related, more);
    // Nothing read at all is an error rather than an empty rail, so the catalog does not keep
    // the empty answer for the rest of the session.
    let (also_like, more) = match (also_like, more) {
        (Err(error), Err(_)) => return Err(error.context("cannot read any recommendations")),
        (Err(error), _) if artist_id.is_none() => return Err(error),
        pair => pair,
    };
    if let Err(error) = &also_like {
        log::warn!("apple: cannot read the related albums: {error:#}");
    }
    let (more_by, similar) = match more {
        Ok(more) => more,
        Err(error) => {
            log::warn!("apple: cannot read more from this artist: {error:#}");
            (Vec::new(), Vec::new())
        }
    };
    let mut seen = HashSet::new();
    let mut liked: Vec<Album> = more_by
        .into_iter()
        .chain(also_like.unwrap_or_default())
        .filter(|album| seen.insert(album.id.clone()))
        .take(SUGGESTIONS)
        .collect();
    let similar_ids: Vec<String> = similar.iter().map(|artist| artist.id.clone()).collect();
    if liked.len() < SUGGESTIONS && !similar_ids.is_empty() {
        for album in similar_releases(client, album_id, &similar_ids).await {
            if liked.len() >= SUGGESTIONS {
                break;
            }
            if seen.insert(album.id.clone()) {
                liked.push(album);
            }
        }
    }
    Ok(AlbumCatalogue {
        also_like: liked,
        similar: similar.into_iter().take(SUGGESTIONS).collect(),
    })
}
