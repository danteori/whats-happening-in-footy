use std::time::Duration;

use reqwest::{
    Client, RequestBuilder, StatusCode,
    header::{HeaderMap, HeaderName, HeaderValue},
    redirect::Policy,
};
use serde::{Deserialize, de::DeserializeOwned};

use crate::{clubs::UnknownTeam, config::ApiKey};

const MESSAGE_LIMIT: usize = 200;
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const USER_AGENT: &str = concat!(
    "whats-happening-in-footy/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/danteori/whats-happening-in-footy)"
);

#[derive(Debug, Clone, thiserror::Error)]
pub enum SourceError {
    #[error("the request to {source_name} failed: {detail}")]
    Request {
        source_name: &'static str,
        detail: String,
    },
    #[error("{source_name} answered with HTTP {status}{}", message_suffix(.message))]
    Status {
        source_name: &'static str,
        status: u16,
        message: Option<String>,
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
        .redirect(Policy::none())
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

pub struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Reply {
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn header_number(&self, name: &str) -> Option<u32> {
        self.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .and_then(|text| text.trim().parse().ok())
    }

    pub fn json<T: DeserializeOwned>(self, source_name: &'static str) -> Result<T, SourceError> {
        if !self.status.is_success() {
            return Err(SourceError::Status {
                source_name,
                status: self.status.as_u16(),
                message: upstream_message(&self.body),
            });
        }
        parse_json(&self.body, source_name)
    }
}

pub async fn send(
    request: RequestBuilder,
    source_name: &'static str,
) -> Result<Reply, SourceError> {
    let request_failed = |error: reqwest::Error| SourceError::Request {
        source_name,
        detail: error.without_url().to_string(),
    };
    let response = request.send().await.map_err(request_failed)?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.bytes().await.map_err(request_failed)?.to_vec();
    Ok(Reply {
        status,
        headers,
        body,
    })
}

pub async fn fetch_json<T: DeserializeOwned>(
    request: RequestBuilder,
    source_name: &'static str,
) -> Result<T, SourceError> {
    send(request, source_name).await?.json(source_name)
}

fn upstream_message(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct ErrorBody {
        message: String,
    }
    let message = serde_json::from_slice::<ErrorBody>(body).ok()?.message;
    let one_line = message.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(one_line.chars().take(MESSAGE_LIMIT).collect()).filter(|text: &String| !text.is_empty())
}

fn message_suffix(message: &Option<String>) -> String {
    message
        .as_ref()
        .map(|text| format!(": {text}"))
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(status: u16, body: &str) -> Reply {
        Reply {
            status: StatusCode::from_u16(status).unwrap(),
            headers: HeaderMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn error_text(reply: Reply) -> String {
        reply
            .json::<serde_json::Value>("football-data.org")
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn an_error_holds_the_upstream_message() {
        let text = error_text(reply(
            400,
            r#"{"message":"Your API token is invalid.","errorCode":400}"#,
        ));

        assert_eq!(
            text,
            "football-data.org answered with HTTP 400: Your API token is invalid."
        );
    }

    #[test]
    fn an_error_without_a_message_holds_only_the_status() {
        for body in ["", "<html>Bad Gateway</html>", r#"{"message":"   "}"#] {
            assert_eq!(
                error_text(reply(502, body)),
                "football-data.org answered with HTTP 502"
            );
        }
    }

    #[test]
    fn a_long_message_is_cut_to_one_short_line() {
        let long = format!(r#"{{"message":"first\nsecond {}"}}"#, "x".repeat(500));

        let text = error_text(reply(403, &long));

        assert!(text.contains("HTTP 403: first second x"));
        assert!(!text.contains('\n'));
        assert!(text.len() < MESSAGE_LIMIT + 50);
    }

    #[test]
    fn a_number_header_is_read() {
        let mut answer = reply(200, "{}");
        answer
            .headers
            .insert("x-ratelimit-requests-remaining", " 42 ".parse().unwrap());

        assert_eq!(
            answer.header_number("x-ratelimit-requests-remaining"),
            Some(42)
        );
        assert_eq!(answer.header_number("missing"), None);
    }
}
