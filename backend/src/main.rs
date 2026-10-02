use std::{env, path::PathBuf};

use backend::app;

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
