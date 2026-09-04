use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::Accessor;
use lofty::probe::Probe;
use lofty::tag::Tag;

use crate::{
    Album, ArtistRef, LOCAL_ALBUM_PREFIX, LOCAL_ARTIST_PREFIX, LOCAL_TRACK_PREFIX, ReleaseType,
    Track,
};

const COVER_NAMES: &[&str] = &[
    "cover.jpg",
    "cover.jpeg",
    "cover.png",
    "cover.webp",
    "folder.jpg",
    "folder.jpeg",
    "folder.png",
    "folder.webp",
];

const ARTIST_NAMES: &[&str] = &[
    "artist.jpg",
    "artist.jpeg",
    "artist.png",
    "artist.webp",
    "folder.jpg",
    "folder.jpeg",
    "folder.png",
    "folder.webp",
    "cover.jpg",
    "cover.jpeg",
    "cover.png",
    "cover.webp",
];

const PLAYABLE_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "wav", "opus",
];

fn is_playable(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PLAYABLE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

pub fn track_id(path: &Path) -> String {
    format!("{LOCAL_TRACK_PREFIX}{}", path.display())
}

pub fn path_from_track_id(id: &str) -> Option<&Path> {
    id.strip_prefix(LOCAL_TRACK_PREFIX).map(Path::new)
}

pub fn album_id(dir: &Path) -> String {
    format!("{LOCAL_ALBUM_PREFIX}{}", dir.display())
}

pub fn path_from_album_id(id: &str) -> Option<&Path> {
    id.strip_prefix(LOCAL_ALBUM_PREFIX).map(Path::new)
}

pub fn artist_id(name: &str) -> String {
    format!("{LOCAL_ARTIST_PREFIX}{name}")
}

pub fn artist_name_from_id(id: &str) -> Option<&str> {
    id.strip_prefix(LOCAL_ARTIST_PREFIX)
}

fn artist_ref(name: &str) -> ArtistRef {
    ArtistRef {
        name: name.to_owned(),
        id: Some(artist_id(name)),
    }
}

fn clean(value: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub fn track_from_file(
    path: &Path,
    artist_hint: Option<&str>,
    album: Option<(&str, &Path)>,
    cache_dir: &Path,
) -> Option<Track> {
    let tagged = Probe::open(path).ok()?.read().ok();
    let tag = tagged
        .as_ref()
        .and_then(|file| file.primary_tag().or_else(|| file.first_tag()));

    let name = clean(tag.and_then(Accessor::title)).unwrap_or_else(|| file_stem(path));
    let artist = clean(tag.and_then(Accessor::artist))
        .or_else(|| artist_hint.map(str::to_owned))
        .unwrap_or_else(|| "Unknown Artist".to_owned());
    let album_name = clean(tag.and_then(Accessor::album))
        .or_else(|| album.map(|(name, _)| name.to_owned()))
        .unwrap_or_default();
    let album_dir = album.map(|(_, dir)| dir);
    let track_number = tag.and_then(Accessor::track).unwrap_or(0);
    let disc_number = tag.and_then(Accessor::disk).unwrap_or(0);
    let duration = tagged
        .as_ref()
        .map(|file| file.properties().duration())
        .unwrap_or_default();
    let cover = extract_cover(tag, path, album_dir, cache_dir);

    Some(Track {
        id: Some(track_id(path)),
        name,
        playable: is_playable(path),
        artists: artist.clone(),
        artist_refs: vec![artist_ref(&artist)],
        album: album_name,
        album_id: album_dir.map(album_id),
        cover,
        duration,
        added_at: None,
        added_by: None,
        playcount: None,
        popularity: 0,
        explicit: false,
        track_number,
        disc_number,
        tags: Vec::new(),
        languages: Vec::new(),
        credits: Vec::new(),
    })
}

pub fn tag_year(path: &Path) -> Option<i32> {
    let tagged = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    tag.date()
        .map(|date| date.year as i32)
        .filter(|year| *year > 0)
}

pub fn album_from_tracks(
    name: &str,
    artist: &str,
    dir: &Path,
    tracks: &[Track],
    year: i32,
) -> Album {
    let cover = folder_cover(dir).or_else(|| tracks.iter().find_map(|track| track.cover.clone()));
    Album {
        id: album_id(dir),
        name: name.to_owned(),
        artists: artist.to_owned(),
        artist_refs: vec![artist_ref(artist)],
        cover: cover.clone(),
        cover_large: cover,
        release_type: ReleaseType::Album,
        year,
        track_count: tracks.len() as u32,
        release_date: String::new(),
        label: String::new(),
        copyrights: Vec::new(),
        added_at: None,
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn folder_cover(dir: &Path) -> Option<String> {
    beside(dir, COVER_NAMES)
}

pub fn artist_cover(dir: &Path) -> Option<String> {
    beside(dir, ARTIST_NAMES)
}

fn beside(dir: &Path, names: &[&str]) -> Option<String> {
    names
        .iter()
        .map(|name| dir.join(name))
        .find(|candidate| candidate.is_file())
        .map(|candidate| format!("file://{}", candidate.display()))
}

fn extract_cover(
    tag: Option<&Tag>,
    path: &Path,
    album_dir: Option<&Path>,
    cache_dir: &Path,
) -> Option<String> {
    if let Some(cover) = album_dir.and_then(folder_cover) {
        return Some(cover);
    }

    let picture = tag.and_then(|tag| {
        tag.pictures()
            .iter()
            .find(|picture| picture.pic_type() == PictureType::CoverFront)
            .or_else(|| tag.pictures().first())
    });

    if let Some(picture) = picture
        && let Some(cached) = cache_picture(picture, cache_dir)
    {
        return Some(cached);
    }

    path.parent().and_then(folder_cover)
}

fn cache_picture(picture: &Picture, cache_dir: &Path) -> Option<String> {
    if picture.data().is_empty() {
        return None;
    }

    let extension = match picture.mime_type() {
        Some(MimeType::Png) => "png",
        Some(MimeType::Gif) => "gif",
        Some(MimeType::Bmp) => "bmp",
        Some(MimeType::Tiff) => "tiff",
        _ => "jpg",
    };

    let mut hasher = DefaultHasher::new();
    picture.data().hash(&mut hasher);
    let hash = hasher.finish();

    let dir = cache_dir.join("local-covers");
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join(format!("{hash:016x}.{extension}"));
    if !dest.exists() {
        std::fs::write(&dest, picture.data()).ok()?;
    }
    Some(format!("file://{}", dest.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::picture::{MimeType, Picture, PictureType};
    use lofty::tag::TagType;

    fn test_dir() -> std::path::PathBuf {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sonora-test-{now}"))
    }

    fn test_pic(data: Vec<u8>, pic_type: PictureType) -> Picture {
        Picture::unchecked(data)
            .pic_type(pic_type)
            .mime_type(MimeType::Jpeg)
            .build()
    }

    #[test]
    fn cache_picture_deduplicates_identical_images() {
        let temp = test_dir();
        let data = vec![1, 2, 3, 4, 5];
        let pic1 = test_pic(data.clone(), PictureType::CoverFront);
        let pic2 = test_pic(data, PictureType::CoverFront);

        let cached1 = cache_picture(&pic1, &temp).unwrap();
        let cached2 = cache_picture(&pic2, &temp).unwrap();

        assert_eq!(cached1, cached2);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn cache_picture_distinguishes_different_images() {
        let temp = test_dir();
        let pic1 = test_pic(vec![1, 2, 3], PictureType::CoverFront);
        let pic2 = test_pic(vec![4, 5, 6], PictureType::CoverFront);

        let cached1 = cache_picture(&pic1, &temp).unwrap();
        let cached2 = cache_picture(&pic2, &temp).unwrap();

        assert_ne!(cached1, cached2);
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn cache_picture_rejects_empty_data() {
        let temp = test_dir();
        let pic = test_pic(Vec::new(), PictureType::CoverFront);

        assert!(cache_picture(&pic, &temp).is_none());
        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn extract_cover_prefers_front_cover() {
        let temp = test_dir();
        let mut tag = Tag::new(TagType::Id3v2);
        let back = test_pic(vec![9, 9, 9], PictureType::CoverBack);
        let front = test_pic(vec![1, 1, 1], PictureType::CoverFront);

        tag.push_picture(back);
        tag.push_picture(front);

        let path = Path::new("test_track.mp3");
        let result = extract_cover(Some(&tag), path, None, &temp).unwrap();
        let front_pic = test_pic(vec![1, 1, 1], PictureType::CoverFront);
        let expected = cache_picture(&front_pic, &temp).unwrap();

        assert_eq!(result, expected);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
