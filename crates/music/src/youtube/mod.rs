mod accounts;
mod auth;
mod client;
mod genres;
mod playback;
mod subscriptions;
mod trim;
mod wire;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use async_trait::async_trait;
use ytmusic::YtMusic;
use ytmusic::browser::{self, Browser, Family};

use crate::youtube::playback::Factory;

use crate::{
    InputSource, MusicProvider, PromptSink, ProviderSession, SignIn, SignInPrompt, UserProfile,
};
pub use client::YouTubeClient;

const GUEST_ID: &str = "youtube-guest";

pub struct YouTubeProvider {
    cookies: PathBuf,
    authuser: PathBuf,
    page_id: PathBuf,
    source: PathBuf,
    guest: PathBuf,
    resolved: PathBuf,
    player: PathBuf,
}

impl YouTubeProvider {
    pub fn new() -> Self {
        let cache = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("sonora")
            .join("youtube");
        Self {
            cookies: cache.join("cookies.txt"),
            authuser: cache.join("authuser.txt"),
            page_id: cache.join("page-id.txt"),
            source: cache.join("browser.txt"),
            guest: cache.join("guest"),
            resolved: cache.join("resolved.json"),
            player: cache.join("player.json"),
        }
    }

    fn cookie_client(&self, cookies: &str, authuser: usize, page_id: Option<&str>) -> Arc<YtMusic> {
        let mut api = YtMusic::with_cookies(cookies).as_user(authuser);
        if let Some(page_id) = page_id {
            api = api.as_brand_account(page_id);
        }
        Arc::new(
            api.cache_resolutions(self.resolved.clone())
                .cache_player(self.player.clone()),
        )
    }

    fn guest_client(&self) -> Arc<YtMusic> {
        Arc::new(YtMusic::anonymous().cache_player(self.player.clone()))
    }

    fn authenticated_session(&self, api: Arc<YtMusic>, profile: UserProfile) -> ProviderSession {
        let client = YouTubeClient::new(api.clone()).owned_by(profile.display_name.clone());
        ProviderSession {
            profile,
            api: Arc::new(client),
            playback: Arc::new(Factory::new(api)),
            authenticated: true,
            playcounts: false,
        }
    }

    fn guest_session(&self, api: Arc<YtMusic>) -> ProviderSession {
        ProviderSession {
            profile: UserProfile {
                id: GUEST_ID.to_string(),
                display_name: "YouTube Music".to_string(),
            },
            api: Arc::new(YouTubeClient::new(api.clone())),
            playback: Arc::new(Factory::new(api)),
            authenticated: false,
            playcounts: false,
        }
    }

    async fn connect(
        &self,
        cookies: &str,
        source: Option<&str>,
        prompt: &PromptSink,
        input: &mut InputSource,
    ) -> Result<ProviderSession> {
        let auth = auth::header(cookies)?;

        if let Some(page_id) = auth.page_id.as_deref() {
            let authuser = auth.authuser.unwrap_or(0);
            let api = self.cookie_client(&auth.cookies, authuser, Some(page_id));
            let profile = api.profile().await.context(
                "the Brand Account headers were accepted, but its YouTube Music profile could not be loaded",
            )?;
            self.store_cookies(&auth.cookies, authuser, Some(page_id), source)?;
            log::debug!("youtube: Brand Account cookie sign-in succeeded for authuser {authuser}");
            return Ok(self.authenticated_session(api, wire::profile(profile)));
        }

        let found = accounts::list(&auth.cookies).await;
        let account = match found.len() {
            0 => anyhow::bail!("cookies were not accepted; sign in to the browser first"),
            1 => &found[0],
            _ => pick(&found, prompt, input).await?,
        };

        let profile = wire::profile(account.profile.clone());
        let api = self.cookie_client(&auth.cookies, account.index, None);
        self.store_cookies(&auth.cookies, account.index, None, source)?;
        log::debug!(
            "youtube: cookie sign-in succeeded for authuser {}",
            account.index
        );
        Ok(self.authenticated_session(api, profile))
    }

    fn store_cookies(
        &self,
        cookies: &str,
        authuser: usize,
        page_id: Option<&str>,
        source: Option<&str>,
    ) -> Result<()> {
        if let Some(parent) = self.cookies.parent() {
            std::fs::create_dir_all(parent).context("cannot create youtube cache dir")?;
        }
        std::fs::write(&self.cookies, cookies).context("cannot store youtube cookies")?;
        std::fs::write(&self.authuser, authuser.to_string())
            .context("cannot store the youtube account")?;
        match page_id {
            Some(page_id) => std::fs::write(&self.page_id, page_id)
                .context("cannot store the youtube Brand Account")?,
            None => {
                let _ = std::fs::remove_file(&self.page_id);
            }
        }
        match source {
            Some(name) => std::fs::write(&self.source, name).context("cannot store the browser")?,
            None => {
                let _ = std::fs::remove_file(&self.source);
            }
        }
        let _ = std::fs::remove_file(&self.guest);
        Ok(())
    }

    async fn restore_cookies(&self) -> Option<ProviderSession> {
        let remembered = self.remembered_browser();
        let live = remembered
            .as_deref()
            .and_then(|name| self.live_cookies(name));
        let cached = std::fs::read_to_string(&self.cookies)
            .ok()
            .map(|cookies| cookies.trim().to_string());
        let authuser = self.stored_authuser();
        let page_id = self.stored_page_id();
        for (fresh, cookies) in [(true, live), (false, cached)] {
            let Some(cookies) = cookies.filter(|cookies| !cookies.is_empty()) else {
                continue;
            };
            let api = self.cookie_client(&cookies, authuser, page_id.as_deref());
            match api.profile().await {
                Ok(profile) => {
                    if fresh {
                        let _ = self.store_cookies(
                            &cookies,
                            authuser,
                            page_id.as_deref(),
                            remembered.as_deref(),
                        );
                    }
                    log::debug!("youtube: restored the session for authuser {authuser}");
                    return Some(self.authenticated_session(api, wire::profile(profile)));
                }
                Err(error) => log::warn!(
                    "youtube: {} cookies are no longer usable: {error:#}",
                    match fresh {
                        true => "the browser",
                        false => "the cached",
                    }
                ),
            }
        }
        None
    }

    fn remembered_browser(&self) -> Option<String> {
        let name = std::fs::read_to_string(&self.source).ok()?;
        let name = name.trim().to_string();
        (!name.is_empty()).then_some(name)
    }

    fn live_cookies(&self, name: &str) -> Option<String> {
        let browser = self
            .browsers()
            .into_iter()
            .find(|browser| browser.name == name)?;
        match auth::cookies(&browser).and_then(|cookies| auth::header(&cookies)) {
            Ok(auth) => Some(auth.cookies),
            Err(error) => {
                log::warn!("youtube: cannot read cookies from {name}: {error:#}");
                None
            }
        }
    }

    fn stored_authuser(&self) -> usize {
        std::fs::read_to_string(&self.authuser)
            .ok()
            .and_then(|stored| stored.trim().parse().ok())
            .unwrap_or(0)
    }

    fn stored_page_id(&self) -> Option<String> {
        std::fs::read_to_string(&self.page_id)
            .ok()
            .map(|stored| stored.trim().to_string())
            .filter(|stored| !stored.is_empty())
    }

    fn store_guest(&self) {
        if let Some(parent) = self.guest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&self.guest, b"");
    }

    fn browsers(&self) -> Vec<Browser> {
        let mut browsers = browser::detect();
        browsers.retain(|browser| browser.family == Family::Firefox);
        #[cfg(target_os = "windows")]
        detect_windows_browsers(&mut browsers);
        browsers.sort_by_key(|browser| browser.name);
        browsers
    }
}

async fn pick<'a>(
    found: &'a [accounts::Account],
    prompt: &PromptSink,
    input: &mut InputSource,
) -> Result<&'a accounts::Account> {
    prompt(SignInPrompt::Accounts(
        found.iter().map(accounts::Account::choice).collect(),
    ));
    let picked = input.recv().await.context("sign-in was cancelled")?;
    found
        .iter()
        .find(|account| account.index.to_string() == picked.trim())
        .context("that account is no longer signed in")
}

#[cfg(target_os = "windows")]
fn detect_windows_browsers(found: &mut Vec<Browser>) {
    const FIREFOX: &[(&str, &str)] = &[
        ("Firefox", "Mozilla/Firefox/Profiles"),
        ("Zen", "zen/Profiles"),
        ("LibreWolf", "librewolf/Profiles"),
        ("Floorp", "Floorp/Profiles"),
        ("Waterfox", "Waterfox/Profiles"),
        ("Mullvad Browser", "Mullvad/MullvadBrowser/Profiles"),
        ("Pale Moon", "Moonchild Productions/Pale Moon/Profiles"),
        ("SeaMonkey", "Mozilla/SeaMonkey/Profiles"),
    ];
    let home = dirs::home_dir();
    let roaming = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.as_ref().map(|home| home.join("AppData/Roaming")));

    if let Some(root) = roaming {
        for &(name, relative) in FIREFOX {
            let root = root.join(relative);
            if has_firefox_cookies(&root) {
                push_browser(found, name, Family::Firefox, root);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn push_browser(found: &mut Vec<Browser>, name: &'static str, family: Family, root: PathBuf) {
    if !found.iter().any(|browser| browser.name == name) {
        found.push(Browser { name, family, root });
    }
}

#[cfg(target_os = "windows")]
fn has_firefox_cookies(root: &std::path::Path) -> bool {
    std::fs::read_dir(root).is_ok_and(|profiles| {
        profiles
            .flatten()
            .any(|profile| profile.path().join("cookies.sqlite").exists())
    })
}

impl Default for YouTubeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MusicProvider for YouTubeProvider {
    fn name(&self) -> &'static str {
        "YouTube Music"
    }

    fn slug(&self) -> &'static str {
        "youtube"
    }

    fn sign_in_options(&self) -> Vec<SignIn> {
        let mut options = vec![SignIn::Anonymous, SignIn::Secret];
        for browser in self.browsers() {
            options.push(SignIn::Browser(browser.name.to_string()));
        }
        options
    }

    fn stored(&self) -> bool {
        self.cookies.exists() || self.guest.exists()
    }

    async fn restore(&self) -> Result<Option<ProviderSession>> {
        if let Some(session) = self.restore_cookies().await {
            return Ok(Some(session));
        }
        if self.guest.exists() {
            log::debug!("youtube: restoring guest session");
            return Ok(Some(self.guest_session(self.guest_client())));
        }
        Ok(None)
    }

    async fn sign_in(
        &self,
        method: SignIn,
        prompt: crate::PromptSink,
        mut input: InputSource,
    ) -> Result<ProviderSession> {
        match method {
            SignIn::Anonymous | SignIn::Default => {
                self.store_guest();
                Ok(self.guest_session(self.guest_client()))
            }
            SignIn::Browser(name) => {
                let browser = self
                    .browsers()
                    .into_iter()
                    .find(|browser| browser.name == name)
                    .with_context(|| format!("{name} is no longer available"))?;
                let cookies = auth::cookies(&browser)?;
                self.connect(&cookies, Some(browser.name), &prompt, &mut input)
                    .await
            }
            SignIn::Secret => {
                prompt(SignInPrompt::Secret);
                let cookies = input.recv().await.context("sign-in was cancelled")?;
                self.connect(&cookies, None, &prompt, &mut input).await
            }
            SignIn::Path(_) => Err(anyhow::anyhow!(
                "youtube does not sign in with a folder path"
            )),
        }
    }

    fn sign_out(&self) {
        for path in [
            &self.cookies,
            &self.authuser,
            &self.page_id,
            &self.source,
            &self.guest,
        ] {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                log::warn!("youtube: cannot remove credential cache: {error}");
            }
        }
    }
}
