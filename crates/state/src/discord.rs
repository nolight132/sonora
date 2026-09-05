mod ipc;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui::{Context, Entity};
use music::{MediaKind, Track};
use serde_json::{Value, json};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use crate::{AppSettings, DiscordLabel, Io, Playback, PlaybackState, Session};

// Public YouTube Music application ID used by pear-devs/pear-desktop
pub(crate) const CLIENT_ID: &str = "1177081335727267940";
const RETRY: Duration = Duration::from_secs(5);
const REFRESH: Duration = Duration::from_secs(15);
const THROTTLE: Duration = Duration::from_secs(1);

pub(crate) struct Discord {
    playback: Entity<Playback>,
    session: Entity<Session>,
    settings: Entity<AppSettings>,
    updates: watch::Sender<Update>,
    worker: JoinHandle<()>,
}

impl Discord {
    pub fn new(
        playback: Entity<Playback>,
        session: Entity<Session>,
        settings: Entity<AppSettings>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        let (updates, receiver) = watch::channel(Update::default());
        cx.observe(&playback, |this, _, cx| this.publish(cx))
            .detach();
        cx.observe(&settings, |this, _, cx| this.publish(cx))
            .detach();
        cx.observe(&session, |this, _, cx| this.publish(cx))
            .detach();
        let this = Self {
            playback,
            session,
            settings,
            updates,
            worker: io.spawn(run(receiver)),
        };
        this.publish(cx);
        this
    }

    fn publish(&self, cx: &Context<Self>) {
        let settings = self.settings.read(cx);
        let client_id = settings
            .discord_rpc()
            .then(|| settings.discord_client_id().to_owned());
        let playback = self.playback.read(cx);
        let playing = client_id.is_some() && *playback.state() == PlaybackState::Playing;
        let presence = playback.track().filter(|_| playing).map(|track| {
            let session = self.session.read(cx);
            let name = application_name(settings.discord_label(), session.provider_name());
            let url = track.id.as_deref().and_then(|id| {
                let client = match music::is_local_id(id) {
                    true => session.local_client(),
                    false => session.client(),
                }?;
                client.share_url(MediaKind::Track, id)
            });
            Presence::new(
                name,
                track.clone(),
                url,
                playback.position(),
                SystemTime::now(),
            )
        });
        self.updates.send_replace(Update {
            client_id,
            presence,
        });
    }
}

impl Drop for Discord {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

#[derive(Clone, Default)]
struct Update {
    client_id: Option<String>,
    presence: Option<Presence>,
}

#[derive(Clone)]
struct Presence {
    name: &'static str,
    track: Track,
    url: Option<String>,
    start: u64,
}

impl Presence {
    fn new(
        name: &'static str,
        track: Track,
        url: Option<String>,
        position: Duration,
        now: SystemTime,
    ) -> Self {
        let elapsed = if track.duration.is_zero() {
            position
        } else {
            position.min(track.duration)
        };
        let start = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_sub(elapsed)
            .as_secs();
        Self {
            name,
            track,
            url,
            start,
        }
    }

    fn unchanged(&self, other: &Self) -> bool {
        self.name == other.name
            && self.track == other.track
            && self.url == other.url
            && self.start.abs_diff(other.start) <= 2
    }

    fn activity(&self) -> Value {
        let mut activity = json!({
            "type": 2,
            "name": self.name,
            "details": activity_text(&self.track.name),
            "state": activity_text(&self.track.artists),
        });
        if !self.track.duration.is_zero() {
            activity["timestamps"] = json!({
                "start": self.start,
                "end": self.start.saturating_add(self.track.duration.as_secs()),
            });
        }
        if let Some(cover) = self.track.cover.as_deref().filter(|url| public_url(url)) {
            activity["assets"] = json!({
                "large_image": cover,
                "large_text": activity_text(&self.track.album),
            });
        }
        if let Some(url) = self.url.as_deref().filter(|url| public_url(url)) {
            activity["buttons"] = json!([{ "label": "Listen", "url": url }]);
        }
        activity
    }
}

fn application_name(label: DiscordLabel, provider: Option<&'static str>) -> &'static str {
    match label {
        DiscordLabel::Sonora => "Sonora",
        DiscordLabel::AutoDetect => provider.unwrap_or("Sonora"),
    }
}

fn public_url(url: &str) -> bool {
    url.len() <= 512 && (url.starts_with("https://") || url.starts_with("http://"))
}

fn activity_text(text: &str) -> String {
    let mut text: String = text.trim().chars().take(128).collect();
    if text.is_empty() {
        return "Sonora".to_owned();
    }
    if text.chars().count() == 1 {
        text.push('\u{2800}');
    }
    text
}

async fn run(mut updates: watch::Receiver<Update>) {
    let mut timer = tokio::time::interval(THROTTLE);
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut connection: Option<ipc::Client> = None;
    let mut client_id = None;
    let mut reported: Option<Presence> = None;
    let mut next_connect = Instant::now();
    let mut sent = Instant::now() - REFRESH;
    loop {
        tokio::select! {
            changed = updates.changed() => if changed.is_err() { return; },
            _ = timer.tick() => {},
        }
        let update = updates.borrow_and_update().clone();
        if update.client_id != client_id {
            if let Some(client) = connection.as_mut() {
                let _ = client.set_activity(Value::Null).await;
            }
            connection = None;
            reported = None;
            client_id = update.client_id;
            next_connect = Instant::now();
        }
        let Some(id) = client_id.as_deref() else {
            continue;
        };
        if connection.is_none() {
            if update.presence.is_none() || Instant::now() < next_connect {
                continue;
            }
            next_connect = Instant::now() + RETRY;
            match ipc::Client::connect(id).await {
                Ok(client) => {
                    connection = Some(client);
                    reported = None;
                    sent = Instant::now() - REFRESH;
                    log::debug!("discord: connected");
                }
                Err(error) => {
                    log::debug!("discord: cannot connect: {error:#}");
                    continue;
                }
            }
            // A pause, sign-out or disable may have arrived during the handshake
            if updates.has_changed().unwrap_or(true) {
                continue;
            }
        }
        let unchanged = match (&update.presence, &reported) {
            (Some(current), Some(previous)) => current.unchanged(previous),
            (None, None) => true,
            _ => false,
        };
        if update.presence.is_some()
            && (sent.elapsed() < THROTTLE || (unchanged && sent.elapsed() < REFRESH))
        {
            continue;
        }
        if update.presence.is_none() && reported.is_none() {
            continue;
        }
        let activity = update
            .presence
            .as_ref()
            .map_or(Value::Null, Presence::activity);
        match connection.as_mut().unwrap().set_activity(activity).await {
            Ok(()) => {
                reported = update.presence;
                sent = Instant::now();
            }
            Err(error) => {
                log::debug!("discord: cannot update activity: {error:#}");
                connection = None;
                reported = None;
                next_connect = Instant::now() + RETRY;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track {
            id: Some("song".into()),
            name: "Title".into(),
            playable: true,
            artists: "Artist".into(),
            artist_refs: vec![],
            album: "Album".into(),
            album_id: None,
            cover: Some("https://example.com/art.jpg".into()),
            duration: Duration::from_secs(180),
            added_at: None,
            added_by: None,
            playcount: None,
            popularity: 0,
            explicit: false,
            track_number: 1,
            disc_number: 1,
            tags: vec![],
            languages: vec![],
            credits: vec![],
        }
    }

    #[test]
    fn activity_has_listening_metadata_and_seconds_based_timestamps() {
        let presence = Presence::new(
            "Sonora",
            track(),
            Some("https://music.youtube.com/watch?v=song".into()),
            Duration::from_secs(30),
            UNIX_EPOCH + Duration::from_secs(1000),
        );
        let activity = presence.activity();
        assert_eq!(activity["type"], 2);
        assert_eq!(activity["name"], "Sonora");
        assert_eq!(activity["details"], "Title");
        assert_eq!(activity["state"], "Artist");
        assert_eq!(activity["timestamps"], json!({"start":970,"end":1150}));
        assert_eq!(activity["assets"]["large_text"], "Album");
        assert_eq!(
            activity["buttons"][0]["url"],
            "https://music.youtube.com/watch?v=song"
        );
    }

    #[test]
    fn local_artwork_and_unknown_durations_are_omitted() {
        let mut track = track();
        track.cover = Some("file:///C:/Music/private.jpg".into());
        track.duration = Duration::ZERO;
        let presence = Presence::new(
            "Sonora",
            track,
            Some("file:///C:/Music/song.flac".into()),
            Duration::ZERO,
            UNIX_EPOCH,
        );
        let activity = presence.activity();
        assert!(activity.get("assets").is_none());
        assert!(activity.get("buttons").is_none());
        assert!(activity.get("timestamps").is_none());
    }

    #[test]
    fn seek_changes_timestamps_but_normal_progress_does_not() {
        let now = UNIX_EPOCH + Duration::from_secs(1000);
        let first = Presence::new("Sonora", track(), None, Duration::from_secs(30), now);
        let tick = Presence::new(
            "Sonora",
            track(),
            None,
            Duration::from_secs(31),
            now + Duration::from_secs(1),
        );
        assert!(first.unchanged(&tick));
        let seek = Presence::new("Sonora", track(), None, Duration::from_secs(60), now);
        assert!(!first.unchanged(&seek));
        let past_end = Presence::new("Sonora", track(), None, Duration::MAX, now);
        assert_eq!(past_end.activity()["timestamps"]["end"], 1000);
        let before_epoch =
            Presence::new("Sonora", track(), None, Duration::from_secs(60), UNIX_EPOCH);
        assert_eq!(before_epoch.start, 0);
    }

    #[test]
    fn application_labels_follow_the_setting_and_connected_provider() {
        for provider in [Some("Spotify"), Some("YouTube Music"), None] {
            assert_eq!(application_name(DiscordLabel::Sonora, provider), "Sonora");
        }
        for (provider, expected) in [
            (Some("Spotify"), "Spotify"),
            (Some("YouTube Music"), "YouTube Music"),
            (None, "Sonora"),
        ] {
            let name = application_name(DiscordLabel::AutoDetect, provider);
            let presence = Presence::new(name, track(), None, Duration::ZERO, UNIX_EPOCH);
            assert_eq!(presence.activity()["name"], expected);
        }
        let sonora = Presence::new("Sonora", track(), None, Duration::ZERO, UNIX_EPOCH);
        let youtube = Presence::new("YouTube Music", track(), None, Duration::ZERO, UNIX_EPOCH);
        assert!(!sonora.unchanged(&youtube));
    }

    #[test]
    fn metadata_limits_preserve_unicode_and_pad_short_titles() {
        assert_eq!(activity_text("  "), "Sonora");
        assert_eq!(activity_text("꿈").chars().count(), 2);
        assert_eq!(activity_text(&"🎵".repeat(150)).chars().count(), 128);
    }
}
