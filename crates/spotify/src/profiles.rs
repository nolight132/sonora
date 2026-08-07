// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use librespot_core::Session;
use tokio::task::JoinSet;

use crate::wire::Named;

pub async fn display_names(
    session: &Session,
    usernames: HashSet<String>,
) -> HashMap<String, String> {
    let mut pending = JoinSet::new();
    for username in usernames {
        let session = session.clone();
        pending.spawn(async move {
            let name = display_name(&session, &username).await;
            (username, name)
        });
    }

    let mut names = HashMap::new();
    while let Some(joined) = pending.join_next().await {
        if let Ok((username, Some(name))) = joined {
            names.insert(username, name);
        }
    }
    names
}

async fn display_name(session: &Session, username: &str) -> Option<String> {
    let body = session
        .spclient()
        .get_user_profile(username, Some(0), Some(0))
        .await
        .inspect_err(|error| log::debug!("cannot resolve profile for {username}: {error}"))
        .ok()?;

    serde_json::from_slice::<Named>(&body)
        .ok()?
        .label()
        .map(str::to_owned)
}
