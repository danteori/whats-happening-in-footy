use std::time::Duration;

use reqwest::{
    Client, RequestBuilder,
    header::{HeaderName, HeaderValue},
};
use serde::de::DeserializeOwned;

use crate::{clubs::UnknownTeam, config::ApiKey};

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const USER_AGENT: &str = concat!(
    "whats-happening-in-footy/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/danteori/whats-happening-in-footy)"
);

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("the request to {source_name} failed: {detail}")]
    Request {
        source_name: &'static str,
        detail: String,
    },
    #[error("{source_name} answered with HTTP {status}")]
    Status {
        source_name: &'static str,
        status: u16,
    },
    #[error("{source_name} sent data in an unexpected shape: {detail}")]
    Shape {
        source_name: &'static str,
        detail: String,
    },
    #[error(transparent)]
    UnknownTeam(#[from] UnknownTeam),
}

pub fn build_client() -> Client {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .expect("the HTTP client configuration is valid")
}

pub fn with_key(
    request: RequestBuilder,
    header: &'static str,
    key: &ApiKey,
    source_name: &'static str,
) -> Result<RequestBuilder, SourceError> {
    let mut value = HeaderValue::from_str(key.expose()).map_err(|_| SourceError::Request {
        source_name,
        detail: "the API key has characters that an HTTP header cannot hold".into(),
    })?;
    value.set_sensitive(true);
    Ok(request.header(HeaderName::from_static(header), value))
}

pub async fn fetch_json<T: DeserializeOwned>(
    request: RequestBuilder,
    source_name: &'static str,
) -> Result<T, SourceError> {
    let request_failed = |error: reqwest::Error| SourceError::Request {
        source_name,
        detail: error.without_url().to_string(),
    };
    let response = request.send().await.map_err(request_failed)?;
    let status = response.status();
    if !status.is_success() {
        return Err(SourceError::Status {
            source_name,
            status: status.as_u16(),
        });
    }
    let body = response.bytes().await.map_err(request_failed)?;
    parse_json(&body, source_name)
}

pub fn parse_json<T: DeserializeOwned>(
    body: &[u8],
    source_name: &'static str,
) -> Result<T, SourceError> {
    serde_json::from_slice(body).map_err(|error| SourceError::Shape {
        source_name,
        detail: error.to_string(),
    })
}
