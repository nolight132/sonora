//! Minting YouTube's proof-of-origin token in a hidden browser window.
//!
//! YouTube stops serving streams to an address it has flagged unless the request carries a token
//! that proves it came from a browser. Google's BotGuard virtual machine produces one only after
//! attesting the engine it is running in, and no hand-written javascript environment passes that,
//! so the app opens the one real engine it ships: a `webview` page on youtube.com running
//! `potoken.js`, off the screen, which leaves the token in a cookie.
//!
//! Nothing here drives it on a schedule. `music::potoken` records the identifier the YouTube
//! client is waiting on, this entity notices, mints once, and files the answer; the client uses a
//! cold start token until then, which covers the start of a track and no more. A host where
//! webkit2gtk does not load never opens a window and never mints.

use std::time::{Duration, Instant};

use gpui::{App, AppContext as _, Context, Task};

/// Where the minting page is loaded from. The binding rides in the query string because reading
/// cookies is the only channel back out of the page.
const PAGE: &str = "https://www.youtube.com/";
const LANDING: &str = "www.youtube.com";
const DOMAIN: &str = "youtube.com";
/// The cookie the page leaves its answer in.
const COOKIE: &str = "SONORA_POT";
/// What the page prefixes a failure with. A token is base64url and never starts with one.
const FAILED: char = '!';
/// How often the entity looks for an ask and polls a window it has open.
const TICK: Duration = Duration::from_millis(500);
/// How long a page gets to attest and mint before it is given up on. Google's round trip takes a
/// second or two; a page still quiet after this is not going to answer.
const PATIENCE: Duration = Duration::from_secs(45);

/// The browser window that mints proof-of-origin tokens, and the one ask it is serving.
pub struct PoToken {
    page: Option<webview::Page>,
    /// What the open page is minting for.
    binding: Option<String>,
    opened: Option<Instant>,
    task: Option<Task<()>>,
}

impl PoToken {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            page: None,
            binding: None,
            opened: None,
            task: None,
        };
        this.watch(cx);
        this
    }

    /// Ticks for the life of the app: one tick starts a mint, carries one along, or does nothing.
    fn watch(&mut self, cx: &mut Context<Self>) {
        self.task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    return;
                }
            }
        }));
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        match self.page.is_some() {
            true => self.carry(),
            false => self.start(cx),
        }
    }

    /// Opens a page for whatever the client is waiting on, if anything.
    fn start(&mut self, _cx: &mut Context<Self>) {
        let Some(binding) = music::potoken::wanted() else {
            return;
        };
        if !webview::supported() {
            log::debug!(
                "potoken: no browser engine here, the stream goes out on a cold start token"
            );
            music::potoken::failed(&binding);
            return;
        }
        let target = webview::Target {
            url: format!("{PAGE}?binding={}", music::escape::component(&binding)),
            landing: LANDING.to_string(),
            domain: DOMAIN.to_string(),
            proof: vec![COOKIE.to_string()],
            title: String::new(),
            agent: None,
            script: Some(include_str!("potoken.js").to_string()),
        };
        match webview::Page::open(target) {
            Ok(page) => {
                log::debug!("potoken: minting for {binding}");
                self.page = Some(page);
                self.binding = Some(binding);
                self.opened = Some(Instant::now());
            }
            Err(error) => {
                log::warn!("potoken: cannot open the minting page: {error:#}");
                music::potoken::failed(&binding);
            }
        }
    }

    /// Carries the open page one poll further, and files whatever it produced.
    fn carry(&mut self) {
        let Some(page) = self.page.as_mut() else {
            return;
        };
        let polled = page.poll();
        let binding = self.binding.clone().unwrap_or_default();
        match polled {
            webview::Poll::Pending => {
                if self.overdue() {
                    log::warn!("potoken: the minting page did not answer in {PATIENCE:?}");
                    self.done(&binding, None);
                }
            }
            webview::Poll::Closed => {
                log::warn!("potoken: the minting page closed before it answered");
                self.done(&binding, None);
            }
            webview::Poll::Cookies(header) => {
                let answer = value(&header, COOKIE).unwrap_or_default();
                match answer.strip_prefix(FAILED) {
                    Some(why) => {
                        log::warn!("potoken: the page could not mint a token: {why}");
                        self.done(&binding, None);
                    }
                    None => {
                        log::debug!("potoken: minted a token for {binding}");
                        self.done(&binding, Some(answer.to_string()));
                    }
                }
            }
        }
    }

    fn overdue(&self) -> bool {
        self.opened.is_some_and(|at| at.elapsed() > PATIENCE)
    }

    /// Files the outcome and closes the page. A failure is filed too, which is what holds the next
    /// attempt off rather than reopening a window every tick.
    fn done(&mut self, binding: &str, token: Option<String>) {
        self.page = None;
        self.binding = None;
        self.opened = None;
        match token {
            Some(token) => music::potoken::give(binding, token),
            None => music::potoken::failed(binding),
        }
    }
}

/// Reads one cookie out of a `Cookie` header value.
fn value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}

/// Starts the minting window. Nothing holds it but the global, and nothing reads it: the client
/// and the window meet in `music::potoken`.
pub fn attach(cx: &mut App) -> gpui::Entity<PoToken> {
    cx.new(PoToken::new)
}
