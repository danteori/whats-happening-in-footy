pub mod clubs;
pub mod config;
pub mod domain;
pub mod football_data;
pub mod highlightly;
pub mod upstream;

use std::path::PathBuf;

use axum::{Json, Router, routing::get};
use serde::Serialize;
use tower_http::services::ServeDir;

#[derive(Serialize)]
struct Hello {
    message: &'static str,
}

async fn hello() -> Json<Hello> {
    Json(Hello {
        message: "Hello, world! (from the Rust backend)",
    })
}

/// API routes live under `/api`; every other path is served from the frontend folder.
pub fn app(frontend_dir: PathBuf) -> Router {
    Router::new()
        .route("/api/hello", get(hello))
        .route("/healthz", get(|| async { "ok" }))
        .fallback_service(ServeDir::new(frontend_dir))
}
