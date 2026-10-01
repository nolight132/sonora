use anyhow::{Context as _, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::Track;

const ALBUMS: &str = "https://api.deezer.com/search/album";
const TRACKS: &str = "https://api.deezer.com/search/track";
const RESULTS: &str = "10";

/// Finds cover art by name in Deezer's public catalogue, which needs no account and serves every
/// image from a public https url. It is for a track whose own art cannot be handed to someone
/// else, so a miss is `None` rather than an error.
#[derive(Clone, Default)]
pub struct ArtworkSearch {
    http: reqwest::Client,
}

/// What a lookup searches for. A track with an album is looked up by the album, so every track
/// of one album shares a query and a result. A track without one is looked up by its title.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ArtworkQuery {
    artist: String,
    album: String,
    title: String,
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(default = "Vec::new")]
    data: Vec<T>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct FoundAlbum {
    title: String,
    artist: Named,
    cover_xl: Option<String>,
}

#[derive(Deserialize)]
struct FoundTrack {
    title: String,
    artist: Named,
    album: Covered,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
struct Covered {
    cover_xl: Option<String>,
}

impl ArtworkQuery {
    /// The query for `track`, or `None` when it has no artist or nothing else to search by.
    pub fn for_track(track: &Track) -> Option<Self> {
        let artist = track
            .artist_refs
            .first()
            .map(|artist| artist.name.as_str())
            .or_else(|| track.artists.split(", ").next())
            .map(searchable)
            .unwrap_or_default();
        let album = searchable(&track.album);
        let title = match album.is_empty() {
            true => searchable(&track.name),
            false => String::new(),
        };
        if artist.is_empty() || album.is_empty() && title.is_empty() {
            return None;
        }
        Some(Self {
            artist,
            album,
            title,
        })
    }
}

impl ArtworkSearch {
    pub fn new() -> Self {
        Self::default()
    }

    /// The url of a cover for `query`, or `None` when no result names the same artist and a
    /// close enough album or title to trust. A wrong cover is worse than none.
    pub async fn find(&self, query: &ArtworkQuery) -> Result<Option<String>> {
        match query.album.is_empty() {
            false => {
                let found: Vec<FoundAlbum> = self.search(ALBUMS, &query.album, query).await?;
                let candidates = found
                    .into_iter()
                    .filter(|album| same_artist(&album.artist.name, &query.artist))
                    .map(|album| (album.title, album.cover_xl));
                Ok(closest(candidates, &query.album))
            }
            true => {
                let found: Vec<FoundTrack> = self.search(TRACKS, &query.title, query).await?;
                let candidates = found
                    .into_iter()
                    .filter(|track| same_artist(&track.artist.name, &query.artist))
                    .map(|track| (track.title, track.album.cover_xl));
                Ok(closest(candidates, &query.title))
            }
        }
    }

    /// One page of results for the artist and `name`. Deezer reports a refused search, such as
    /// one over its rate limit, in the body of a successful response.
    async fn search<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        name: &str,
        query: &ArtworkQuery,
    ) -> Result<Vec<T>> {
        let terms = format!("{} {name}", query.artist);
        let response = self
            .http
            .get(endpoint)
            .query(&[("q", terms.as_str()), ("limit", RESULTS)])
            .send()
            .await
            .context("cannot reach deezer")?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!("deezer answered with status {status}");
        }
        let page: Page<T> = response
            .json()
            .await
            .context("cannot read the deezer search")?;
        if let Some(error) = page.error {
            anyhow::bail!("deezer refused the search: {error}");
        }
        Ok(page.data)
    }
}

/// The cover of the candidate whose name matches `wanted` exactly, or failing that the first one
/// that only adds words to it, as an edition or a feature credit does.
fn closest(
    candidates: impl Iterator<Item = (String, Option<String>)>,
    wanted: &str,
) -> Option<String> {
    let wanted = compared(wanted);
    let mut near = None;
    for (name, cover) in candidates {
        let Some(cover) = cover.filter(|cover| cover.starts_with("https://")) else {
            continue;
        };
        let name = compared(&searchable(&name));
        if name == wanted {
            return Some(cover);
        }
        if near.is_none() && extends(&name, &wanted) {
            near = Some(cover);
        }
    }
    near
}

fn same_artist(found: &str, wanted: &str) -> bool {
    let found = compared(found);
    let wanted = compared(wanted);
    found == wanted || extends(&found, &wanted)
}

/// Whether one of two compared names is the other with whole words added after it.
fn extends(one: &str, other: &str) -> bool {
    let starts = |long: &str, short: &str| {
        !short.is_empty()
            && long
                .strip_prefix(short)
                .is_some_and(|rest| rest.starts_with(' '))
    };
    starts(one, other) || starts(other, one)
}

/// `text` without anything in brackets, since Deezer wants every word of a search to match and
/// "(Official Video)" or "[Remastered]" would rule out the release itself.
fn searchable(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut depth = 0usize;
    for character in text.chars() {
        match character {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => kept.push(character),
            _ => {}
        }
    }
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` lowercased, with every run of punctuation and spaces turned into one space.
fn compared(text: &str) -> String {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}
