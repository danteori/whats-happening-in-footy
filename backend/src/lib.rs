pub mod api;
pub mod budget;
pub mod cache;
pub mod clubs;
pub mod config;
pub mod domain;
pub mod football_data;
pub mod highlightly;
pub mod match_link;
pub mod service;
pub mod table;
pub mod upstream;

use std::path::PathBuf;

use axum::{Json, Router, routing::get};
use serde::Serialize;
use tower_http::services::ServeDir;

use crate::service::DataService;

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
pub fn app(frontend_dir: PathBuf, data: DataService) -> Router {
    Router::new()
        .route("/api/hello", get(hello))
        .route("/healthz", get(|| async { "ok" }))
        .merge(api::routes())
        .fallback_service(ServeDir::new(frontend_dir))
        .with_state(data)
}
