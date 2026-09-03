use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use md5::{Digest, Md5};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

const ENDPOINT: &str = "https://ws.audioscrobbler.com/2.0/";
const AUTHORIZE: &str = "https://www.last.fm/api/auth/";
const PORT: u16 = 8990;
const MIN_LENGTH: Duration = Duration::from_secs(30);
const FULL_PLAY: Duration = Duration::from_secs(240);

pub const API_ACCOUNT_URL: &str = "https://www.last.fm/api/account/create";

const PAGE: &str = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
    <html><body>Sonora is connected to Last.fm. You can close this window.</body></html>";

fn redirect_uri() -> String {
    format!("http://127.0.0.1:{PORT}/lastfm")
}

pub fn authorize_url(api_key: &str) -> String {
    format!(
        "{AUTHORIZE}?api_key={}&cb={}",
        escaped(api_key),
        escaped(&redirect_uri())
    )
}

#[derive(Clone, Debug)]
pub struct Play {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration: Duration,
}

impl Play {
    pub fn earned(&self, played: Duration) -> bool {
        self.duration >= MIN_LENGTH
            && (played >= FULL_PLAY || played.as_secs_f64() * 2. >= self.duration.as_secs_f64())
    }

    fn form(&self) -> Vec<(&'static str, String)> {
        let mut form = vec![
            ("artist", self.artist.clone()),
            ("track", self.title.clone()),
            ("duration", self.duration.as_secs().to_string()),
        ];
        if let Some(album) = self.album.as_deref().filter(|album| !album.is_empty()) {
            form.push(("album", album.to_owned()));
        }
        form
    }
}

pub struct Scrobbler {
    key: String,
    secret: String,
    session: String,
    http: reqwest::Client,
}

impl Scrobbler {
    pub fn new(
        key: impl Into<String>,
        secret: impl Into<String>,
        session: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            secret: secret.into(),
            session: session.into(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn now_playing(&self, play: &Play) -> Result<()> {
        let mut form = play.form();
        form.push(("method", "track.updateNowPlaying".to_owned()));
        self.call(form).await.map(|_| ())
    }

    pub async fn scrobble(&self, play: &Play, at: i64) -> Result<()> {
        let mut form = play.form();
        form.push(("method", "track.scrobble".to_owned()));
        form.push(("timestamp", at.to_string()));
        self.call(form).await.map(|_| ())
    }

    async fn call(&self, mut form: Vec<(&'static str, String)>) -> Result<Answer> {
        form.push(("api_key", self.key.clone()));
        form.push(("sk", self.session.clone()));
        post(&self.http, &self.secret, form).await
    }
}

pub async fn session(key: &str, secret: &str, token: &str) -> Result<(String, String)> {
    let form = vec![
        ("method", "auth.getSession".to_owned()),
        ("api_key", key.to_owned()),
        ("token", token.to_owned()),
    ];
    let answer = post(&reqwest::Client::new(), secret, form).await?;
    let Some(session) = answer.session else {
        bail!("last.fm returned no session");
    };
    Ok((session.key, session.name))
}

pub async fn token(wait: Duration) -> Result<String> {
    let listener = TcpListener::bind(("127.0.0.1", PORT))
        .await
        .context("cannot listen for the last.fm callback")?;
    let deadline = tokio::time::Instant::now() + wait;

    loop {
        let accepted = tokio::time::timeout_at(deadline, listener.accept())
            .await
            .context("last.fm did not call back in time")?;
        let (mut stream, _) = accepted.context("cannot accept the last.fm callback")?;

        let mut line = String::new();
        let read = BufReader::new(&mut stream).read_line(&mut line).await;
        if let Err(error) = read {
            log::warn!("lastfm: cannot read the callback: {error}");
            continue;
        }
        let found = parameter(&line, "token");
        stream.write_all(PAGE.as_bytes()).await.ok();
        if let Some(token) = found {
            return Ok(token);
        }
    }
}

#[derive(Deserialize)]
struct Answer {
    error: Option<u32>,
    message: Option<String>,
    session: Option<Granted>,
}

#[derive(Deserialize)]
struct Granted {
    key: String,
    name: String,
}

async fn post(
    http: &reqwest::Client,
    secret: &str,
    mut form: Vec<(&'static str, String)>,
) -> Result<Answer> {
    form.sort_by(|left, right| left.0.cmp(right.0));
    form.push(("api_sig", signature(&form, secret)));
    form.push(("format", "json".to_owned()));

    let answer: Answer = http
        .post(ENDPOINT)
        .form(&form)
        .send()
        .await
        .context("cannot reach last.fm")?
        .json()
        .await
        .context("cannot read the last.fm answer")?;

    if let Some(code) = answer.error {
        let message = answer.message.unwrap_or_default();
        bail!("last.fm refused the request ({code}): {message}");
    }
    Ok(answer)
}

fn signature(form: &[(&'static str, String)], secret: &str) -> String {
    let mut base = String::new();
    for (name, value) in form {
        base.push_str(name);
        base.push_str(value);
    }
    base.push_str(secret);
    format!("{:x}", Md5::digest(base.as_bytes()))
}

fn escaped(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

fn parameter(request: &str, name: &str) -> Option<String> {
    let (_, query) = request.split_whitespace().nth(1)?.split_once('?')?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode_str(value).decode_utf8_lossy().into_owned())
    })
}
