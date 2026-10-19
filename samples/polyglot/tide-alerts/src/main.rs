//! tide-alerts — the alert feature's HTTP API, served by axum.
mod alert;
mod store;

use axum::{routing::get, Json, Router};
use std::net::SocketAddr;

#[derive(serde::Serialize)]
struct Health {
    status: String,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok".into() })
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/health", get(health))
        .route("/alerts/:berth", get(alert::current_alert))
        .route("/alerts/:berth/check", get(alert::check));
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
