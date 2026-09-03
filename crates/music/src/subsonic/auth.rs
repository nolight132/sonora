use std::path::PathBuf;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub server: String,
    pub username: String,
    pub password: String,
}

pub fn dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("sonora")
        .join("subsonic")
}

pub fn path() -> PathBuf {
    dir().join("credentials.json")
}

pub fn normalize_server(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        anyhow::bail!("the server address is empty");
    }
    let with_scheme = match trimmed.contains("://") {
        true => trimmed.to_owned(),
        false => format!("http://{trimmed}"),
    };
    Ok(with_scheme)
}

pub fn load() -> Option<Credentials> {
    let bytes = std::fs::read(path()).ok()?;
    let mut credentials: Credentials = serde_json::from_slice(&bytes).ok()?;
    credentials.server = normalize_server(&credentials.server).ok()?;
    match credentials.username.is_empty() {
        true => None,
        false => Some(credentials),
    }
}

pub fn store(credentials: &Credentials) -> Result<()> {
    let dir = dir();
    std::fs::create_dir_all(&dir).context("cannot create subsonic cache dir")?;
    let bytes =
        serde_json::to_vec_pretty(credentials).context("cannot serialize subsonic credentials")?;
    std::fs::write(path(), &bytes).context("cannot store subsonic credentials")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(path(), std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn forget() {
    let _ = std::fs::remove_file(path());
}
