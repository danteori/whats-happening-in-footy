use std::path::PathBuf;

use axum::{body::Body, http::Request};
use backend::app;
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
