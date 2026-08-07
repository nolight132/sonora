// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 nolight132

use anyhow::{Context as _, Result, bail};
use bytes::Bytes;
use http::{Method, Request, header};
use librespot_core::{Session, spclient::CLIENT_TOKEN};
use serde::Deserialize;
use serde::de::DeserializeOwned;

mod album;
mod plays;

pub(crate) use album::album;
pub(crate) use plays::track;

const ENDPOINT: &str = "https://api-partner.spotify.com/pathfinder/v2/query";

#[derive(Deserialize)]
struct Response<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphqlError>,
}

#[derive(Deserialize)]
struct GraphqlError {
    message: String,
}

async fn query<T: DeserializeOwned>(
    session: &Session,
    operation: &str,
    hash: &str,
    variables: serde_json::Value,
) -> Result<T> {
    let body = serde_json::to_vec(&serde_json::json!({
        "operationName": operation,
        "variables": variables,
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": hash,
            }
        },
    }))
    .with_context(|| format!("cannot encode {operation} Pathfinder request"))?;
    let token = session
        .login5()
        .auth_token()
        .await
        .context("cannot obtain Spotify access token")?;
    let client_token = session
        .spclient()
        .client_token()
        .await
        .context("cannot obtain Spotify client token")?;
    let request = Request::builder()
        .method(Method::POST)
        .uri(ENDPOINT)
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::AUTHORIZATION,
            format!("{} {}", token.token_type, token.access_token),
        )
        .header(CLIENT_TOKEN, client_token)
        .body(Bytes::from(body))
        .with_context(|| format!("cannot build {operation} Pathfinder request"))?;
    let body = session
        .http_client()
        .request_body(request)
        .await
        .with_context(|| format!("cannot request {operation} from Pathfinder"))?;
    decoded(&body, operation)
}

fn decoded<T: DeserializeOwned>(bytes: &[u8], operation: &str) -> Result<T> {
    let response: Response<T> = serde_json::from_slice(bytes)
        .with_context(|| format!("cannot decode {operation} Pathfinder response"))?;
    if !response.errors.is_empty() {
        let messages = response
            .errors
            .into_iter()
            .map(|error| error.message)
            .collect::<Vec<_>>()
            .join("; ");
        bail!("Spotify rejected {operation} Pathfinder query: {messages}");
    }
    response
        .data
        .with_context(|| format!("{operation} Pathfinder response has no data"))
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::decoded;

    #[test]
    fn decodes_data() {
        let data: Value = decoded(br#"{"data":{"value":42}}"#, "test").unwrap();
        assert_eq!(data["value"], 42);
    }

    #[test]
    fn reports_graphql_error() {
        let error = decoded::<Value>(
            br#"{"data":null,"errors":[{"message":"bad hash"}]}"#,
            "test",
        )
        .unwrap_err();
        assert!(error.to_string().contains("bad hash"));
    }
}
