use std::{env, path::PathBuf};

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
fn app(frontend_dir: PathBuf) -> Router {
    Router::new()
        .route("/api/hello", get(hello))
        .route("/healthz", get(|| async { "ok" }))
        .fallback_service(ServeDir::new(frontend_dir))
}

#[tokio::main]
async fn main() {
    // Render sets PORT and we set HOST=0.0.0.0 there; locally we default to
    // 127.0.0.1 so Windows/macOS don't pop up a firewall prompt.
    let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = env::var("PORT").unwrap_or_else(|_| "3000".into());
    let frontend_dir = env::var("FRONTEND_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend")));

    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}"))
        .await
        .expect("failed to bind address");
    println!("Listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app(frontend_dir))
        .await
        .expect("server error");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn hello_returns_greeting() {
        let response = app(PathBuf::from("../frontend"))
            .oneshot(Request::get("/api/hello").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert!(response.status().is_success());
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("Hello, world!"));
    }
}
