use std::{env, path::PathBuf};

use backend::{
    app,
    config::{Config, FOOTBALL_DATA_API_KEY, HIGHLIGHTLY_API_KEY},
    service::DataService,
    upstream::build_client,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // Render sets PORT and we set HOST=0.0.0.0 there; locally we default to
    // 127.0.0.1 so Windows/macOS don't pop up a firewall prompt.
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = env::var("PORT").unwrap_or_else(|_| "3000".into());
    let frontend_dir = env::var("FRONTEND_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend")));

    let config = Config::from_env();
    warn_about_missing_keys(&config);
    let data = DataService::new(config, build_client());

    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}"))
        .await
        .expect("failed to bind address");
    println!("Listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app(frontend_dir, data))
        .await
        .expect("server error");
}

fn warn_about_missing_keys(config: &Config) {
    let keys = [
        (
            FOOTBALL_DATA_API_KEY,
            config.football_data.api_key.is_some(),
        ),
        (HIGHLIGHTLY_API_KEY, config.highlightly.api_key.is_some()),
    ];
    for (variable, _) in keys.iter().filter(|(_, is_set)| !is_set) {
        tracing::warn!("{variable} is not set; the routes that need it answer HTTP 503");
    }
}
