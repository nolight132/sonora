mod audio;
pub mod binimum;
pub mod kugou;
#[cfg(test)]
mod live_tests;
pub mod local;
pub mod lrclib;
pub mod lyrics;
mod models;
pub mod musixmatch;
pub mod netease;
mod spectrum;
pub mod spotify;
pub mod youtube;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;

pub use models::{
    Album, AlbumDetail, Artist, ArtistProfile, ArtistRef, Contributor, Credit, Genre, GenreDetail,
    GenreItem, GenreSection, HomeFeed, Lyrics, LyricsHit, LyricsLane, LyricsLine, LyricsQuery,
    LyricsWord, Playlist, PlaylistDetail, ReleaseType, RomanizedText, SavedArtist, Track, TrackKey,
    TrackTags, UserDetail, UserProfile, Voice, WritingSystem,
};
pub use spectrum::Spectrum;

pub const LOCAL_TRACK_PREFIX: &str = "local:";
pub const LOCAL_ALBUM_PREFIX: &str = "local-album:";
pub const LOCAL_ARTIST_PREFIX: &str = "local-artist:";
pub const LOCAL_PLAYLIST_PREFIX: &str = "local-playlist:";

pub fn is_local_id(id: &str) -> bool {
    id.starts_with(LOCAL_TRACK_PREFIX)
        || id.starts_with(LOCAL_ALBUM_PREFIX)
        || id.starts_with(LOCAL_ARTIST_PREFIX)
        || id.starts_with(LOCAL_PLAYLIST_PREFIX)
}

pub fn distinct_covers(tracks: &[Track], wanted: usize) -> Vec<String> {
    let mut covers: Vec<String> = Vec::with_capacity(wanted);
    for cover in tracks.iter().filter_map(|track| track.cover.as_deref()) {
        if covers.len() == wanted {
            break;
        }
        if !covers.iter().any(|kept| kept == cover) {
            covers.push(cover.to_owned());
        }
    }

    covers
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Track,
    Album,
    Artist,
    Playlist,
}

#[async_trait]
pub trait MusicApi: Send + Sync {
    fn alive(&self) -> bool {
        true
    }

    fn share_url(&self, kind: MediaKind, id: &str) -> Option<String>;
    async fn profile(&self) -> Result<UserProfile>;

    async fn user(&self, _user_id: &str) -> Result<UserDetail> {
        anyhow::bail!("user profiles are not supported")
    }

    async fn artist(&self, artist_id: &str) -> Result<Artist>;
    async fn artist_profile(&self, artist_id: &str) -> Result<ArtistProfile>;
    async fn artist_images(&self, ids: Vec<String>) -> Result<HashMap<String, String>>;
    async fn saved_tracks(&self, limit: u32) -> Result<Vec<Track>>;

    async fn all_tracks(&self, limit: u32) -> Result<Vec<Track>> {
        self.saved_tracks(limit).await
    }

    async fn set_track_saved(&self, track_id: &str, saved: bool) -> Result<()>;

    /// Reads what the file itself says, for a provider whose tracks are files.
    async fn track_tags(&self, _track_id: &str) -> Result<TrackTags> {
        anyhow::bail!("this provider cannot edit tags")
    }

    async fn set_track_tags(&self, _track_id: &str, _tags: TrackTags) -> Result<()> {
        anyhow::bail!("this provider cannot edit tags")
    }
    async fn track(&self, track_id: &str) -> Result<Track>;
    async fn track_playcount(&self, track_id: &str) -> Result<Option<u64>>;
    async fn track_lyrics(&self, _track_id: &str) -> Result<Option<Lyrics>> {
        Ok(None)
    }
    async fn playlists(&self, limit: u32) -> Result<Vec<Playlist>>;
    async fn create_playlist(&self, name: &str) -> Result<String>;
    async fn rename_playlist(&self, playlist_id: &str, name: &str) -> Result<()>;
    async fn delete_playlist(&self, playlist_id: &str) -> Result<()>;
    async fn remove_playlist_from_library(&self, playlist_id: &str) -> Result<()>;
    async fn add_playlist_to_library(&self, playlist_id: &str) -> Result<()>;
    async fn set_playlist_public(&self, playlist_id: &str, public: bool) -> Result<()>;
    async fn add_track_to_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()>;
    async fn remove_track_from_playlist(&self, playlist_id: &str, track_id: &str) -> Result<()>;
    async fn saved_albums(&self, limit: u32) -> Result<Vec<Album>>;
    async fn set_album_saved(&self, album_id: &str, saved: bool) -> Result<()>;
    async fn saved_artists(&self, limit: u32) -> Result<Vec<SavedArtist>>;
    async fn set_artist_saved(&self, artist_id: &str, saved: bool) -> Result<()>;
    async fn album(&self, album_id: &str) -> Result<AlbumDetail>;
    async fn album_tracks(&self, album_id: &str) -> Result<Vec<Track>>;
    async fn playlist(&self, playlist_id: &str) -> Result<PlaylistDetail>;
    async fn playlist_tracks(&self, playlist_id: &str) -> Result<Vec<Track>>;
    async fn playlist_covers(&self, playlist_id: &str, wanted: usize) -> Result<Vec<String>>;
    async fn track_radio(&self, track_id: &str) -> Result<Vec<Track>>;
    async fn search(&self, query: &str) -> Result<Vec<Track>>;

    async fn search_albums(&self, _query: &str) -> Result<Vec<Album>> {
        Ok(Vec::new())
    }

    async fn search_playlists(&self, _query: &str) -> Result<Vec<Playlist>> {
        Ok(Vec::new())
    }

    async fn home(&self) -> Result<HomeFeed> {
        Ok(HomeFeed::default())
    }

    async fn name_home_playlists(&self, sections: Vec<GenreSection>) -> Vec<GenreSection> {
        sections
    }

    async fn genres(&self) -> Result<Vec<Genre>> {
        Ok(Vec::new())
    }

    async fn genre(&self, _genre_id: &str) -> Result<GenreDetail> {
        Ok(GenreDetail::default())
    }
}

#[async_trait]
pub trait LyricsProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn search(&self, query: &LyricsQuery) -> Result<Vec<LyricsHit>>;
}

#[derive(Clone, Copy, Debug)]
pub struct PlaybackConfig {
    pub normalisation: bool,
    pub gapless: bool,
    pub position_interval: Duration,
    pub gain: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlaybackEvent {
    Loading(Duration),
    Playing(Duration),
    Paused(Duration),
    Position(Duration),
    Length(Duration),
    Ended,
    Unavailable,
    Refused,
    Gated,
    OutputChanged,
}

pub trait Player: Send + Sync {
    fn load(&self, track_id: &str, seamless: bool) -> Result<()>;

    fn load_paused_at(&self, track_id: &str, at: Duration) -> Result<()> {
        self.load(track_id, false)?;
        self.pause();
        self.seek(at);
        Ok(())
    }

    fn preload(&self, track_id: &str) -> Result<()>;
    fn play(&self);
    fn pause(&self);
    fn seek(&self, position: Duration);
    fn set_gain(&self, gain: f32);

    fn spectrum(&self) -> Option<Spectrum> {
        None
    }
}

#[async_trait]
pub trait PlaybackEvents: Send {
    async fn next(&mut self) -> Option<PlaybackEvent>;
}

pub trait PlaybackFactory: Send + Sync {
    fn start(&self, config: PlaybackConfig) -> (Box<dyn Player>, Box<dyn PlaybackEvents>);
}

pub const REMOTE_NAME: &str = "Sonora";
pub const TRACK_PREFIX: &str = "spotify:track:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteKind {
    Computer,
    Phone,
    Speaker,
    Screen,
    Car,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub kind: RemoteKind,
    /// `None` when the device refuses remote volume changes.
    pub volume: Option<f32>,
    pub active: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RemoteState {
    pub own_id: String,
    pub devices: Vec<RemoteDevice>,
    pub active: Option<String>,
    pub track: Option<String>,
    pub playing: bool,
    pub buffering: bool,
    pub position: Duration,
    pub duration: Duration,
    pub shuffle: bool,
}

impl RemoteState {
    pub fn device(&self, id: &str) -> Option<&RemoteDevice> {
        self.devices.iter().find(|device| device.id == id)
    }

    pub fn active_device(&self) -> Option<&RemoteDevice> {
        self.active.as_deref().and_then(|id| self.device(id))
    }
}

/// What another device asked Sonora to do, once Sonora is the one playing.
#[derive(Clone, Debug, PartialEq)]
pub enum HostCommand {
    Take(Handover),
    Play,
    Pause,
    Next,
    Previous,
    Seek(Duration),
    Shuffle(bool),
    Repeat(HostRepeat),
    Volume(f32),
    Enqueue(String),
    Resign,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HostRepeat {
    #[default]
    Off,
    Context,
    Track,
}

/// A transfer: where the handing device was, so Sonora can pick the song up mid-play.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Handover {
    pub track: Option<String>,
    /// Where in the context to start when no track was named — a playlist handed over without a
    /// chosen track arrives this way.
    pub index: Option<usize>,
    pub context: Option<String>,
    pub position: Duration,
    pub paused: bool,
    pub shuffle: bool,
}

/// What Sonora publishes about itself while it holds playback. Every other device's UI is
/// drawn from this, so it has to follow each transport change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostState {
    pub track: Option<String>,
    pub context: Option<String>,
    pub position: Duration,
    pub duration: Duration,
    pub playing: bool,
    pub buffering: bool,
    pub shuffle: bool,
    pub repeat: Option<HostRepeat>,
    pub volume: f32,
    pub past: Vec<String>,
    pub upcoming: Vec<String>,
}

/// Controls playback happening on another device. Every method names the device it targets,
/// so nothing here assumes an active one.
#[async_trait]
pub trait RemoteTransport: Send + Sync {
    fn device_id(&self) -> String;
    async fn refresh(&self) -> Result<RemoteState>;
    async fn play(&self, target: &str) -> Result<()>;
    async fn pause(&self, target: &str) -> Result<()>;
    async fn next(&self, target: &str) -> Result<()>;
    async fn previous(&self, target: &str) -> Result<()>;
    async fn seek(&self, target: &str, position: Duration) -> Result<()>;
    async fn set_volume(&self, target: &str, level: f32) -> Result<()>;
    async fn play_track(&self, target: &str, track_id: &str, at: Duration) -> Result<()>;
    async fn transfer(&self, active: Option<&str>, target: &str) -> Result<()>;
    async fn release(&self) -> Result<()>;

    /// Claims playback and publishes what Sonora is doing. Announcing an active device with a
    /// stale state is what leaves a phone stuck on "Connecting", so this runs after every change.
    async fn publish(&self, state: &HostState) -> Result<()>;

    /// Hands playback back to Spotify without withdrawing the device.
    async fn resign(&self) -> Result<()>;
}

#[async_trait]
pub trait RemoteUpdates: Send {
    async fn next(&mut self) -> Option<RemoteState>;
}

#[async_trait]
pub trait HostCommands: Send {
    async fn next(&mut self) -> Option<HostCommand>;
}

pub struct Remote {
    pub transport: Arc<dyn RemoteTransport>,
    pub updates: Box<dyn RemoteUpdates>,
    pub commands: Box<dyn HostCommands>,
}

#[async_trait]
pub trait RemoteFactory: Send + Sync {
    /// `playable` decides whether Sonora offers itself as a playback target. A control-only
    /// watch stays hidden from every other device's list.
    async fn watch(&self, playable: bool) -> Result<Remote>;
}

pub struct ProviderSession {
    pub profile: UserProfile,
    pub api: Arc<dyn MusicApi>,
    pub playback: Arc<dyn PlaybackFactory>,
    pub remotes: Option<Arc<dyn RemoteFactory>>,
    pub authenticated: bool,
    pub playcounts: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignIn {
    Default,
    Anonymous,
    Browser(String),
    Secret,
    Path(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInProblem {
    Premium,
    Region,
    Credentials,
    Network,
    Cancelled,
    Refused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignInFailure(pub SignInProblem);

impl std::fmt::Display for SignInFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let reason = match self.0 {
            SignInProblem::Premium => "the account has no Spotify Premium",
            SignInProblem::Region => "the account is out of its home region",
            SignInProblem::Credentials => "the stored credentials are no longer valid",
            SignInProblem::Network => "Spotify could not be reached",
            SignInProblem::Cancelled => "authorization was cancelled in the browser",
            SignInProblem::Refused => "Spotify refused the session",
        };
        write!(f, "{reason}")
    }
}

impl std::error::Error for SignInFailure {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountChoice {
    pub id: String,
    pub name: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignInPrompt {
    Accounts(Vec<AccountChoice>),
    Code { code: String, url: String },
    Secret,
}

pub type PromptSink = Arc<dyn Fn(SignInPrompt) + Send + Sync>;
pub type InputSource = tokio::sync::mpsc::UnboundedReceiver<String>;

#[async_trait]
pub trait MusicProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn slug(&self) -> &'static str;
    fn sign_in_options(&self) -> Vec<SignIn>;
    fn stored(&self) -> bool;
    fn location(&self) -> Option<String> {
        None
    }
    async fn restore(&self) -> Result<Option<ProviderSession>>;
    async fn sign_in(
        &self,
        method: SignIn,
        prompt: PromptSink,
        input: InputSource,
    ) -> Result<ProviderSession>;
    fn abandon(&self) {}
    fn sign_out(&self);
}
