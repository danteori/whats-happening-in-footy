use std::{env, fmt};

pub const FOOTBALL_DATA_API_KEY: &str = "FOOTBALL_DATA_API_KEY";
pub const FOOTBALL_DATA_BASE_URL: &str = "FOOTBALL_DATA_BASE_URL";
pub const HIGHLIGHTLY_API_KEY: &str = "HIGHLIGHTLY_API_KEY";
pub const HIGHLIGHTLY_BASE_URL: &str = "HIGHLIGHTLY_BASE_URL";

pub const DEFAULT_FOOTBALL_DATA_BASE_URL: &str = "https://api.football-data.org/v4";
pub const DEFAULT_HIGHLIGHTLY_BASE_URL: &str = "https://soccer.highlightly.net";
pub const HIGHLIGHTLY_DAILY_BUDGET: u32 = 90;

#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiKey(<hidden>)")
    }
}

#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub base_url: String,
    pub api_key: Option<ApiKey>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub football_data: SourceConfig,
    pub highlightly: SourceConfig,
    pub highlightly_daily_budget: u32,
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_lookup(|name| env::var(name).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let source = |key_var: &str, url_var: &str, default_url: &str| SourceConfig {
            base_url: non_empty(lookup(url_var))
                .map(|url| url.trim_end_matches('/').to_owned())
                .unwrap_or_else(|| default_url.to_owned()),
            api_key: non_empty(lookup(key_var)).map(ApiKey::new),
        };
        Self {
            football_data: source(
                FOOTBALL_DATA_API_KEY,
                FOOTBALL_DATA_BASE_URL,
                DEFAULT_FOOTBALL_DATA_BASE_URL,
            ),
            highlightly: source(
                HIGHLIGHTLY_API_KEY,
                HIGHLIGHTLY_BASE_URL,
                DEFAULT_HIGHLIGHTLY_BASE_URL,
            ),
            highlightly_daily_budget: HIGHLIGHTLY_DAILY_BUDGET,
        }
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_from(pairs: &[(&str, &str)]) -> Config {
        Config::from_lookup(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn defaults_apply_without_variables() {
        let config = config_from(&[]);

        assert_eq!(
            config.football_data.base_url,
            DEFAULT_FOOTBALL_DATA_BASE_URL
        );
        assert_eq!(config.highlightly.base_url, DEFAULT_HIGHLIGHTLY_BASE_URL);
        assert!(config.football_data.api_key.is_none());
        assert!(config.highlightly.api_key.is_none());
        assert_eq!(config.highlightly_daily_budget, 90);
    }

    #[test]
    fn variables_override_defaults() {
        let config = config_from(&[
            (FOOTBALL_DATA_API_KEY, " fd-key "),
            (FOOTBALL_DATA_BASE_URL, "http://127.0.0.1:9000/fd/"),
            (HIGHLIGHTLY_API_KEY, "hl-key"),
        ]);

        assert_eq!(config.football_data.base_url, "http://127.0.0.1:9000/fd");
        assert_eq!(config.football_data.api_key, Some(ApiKey::new("fd-key")));
        assert_eq!(config.highlightly.api_key, Some(ApiKey::new("hl-key")));
    }

    #[test]
    fn empty_key_counts_as_missing() {
        let config = config_from(&[(FOOTBALL_DATA_API_KEY, "  ")]);

        assert!(config.football_data.api_key.is_none());
    }

    #[test]
    fn debug_output_hides_the_key() {
        let config = config_from(&[(FOOTBALL_DATA_API_KEY, "secret-value")]);

        assert!(!format!("{config:?}").contains("secret-value"));
    }
}
