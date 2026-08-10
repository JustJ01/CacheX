

use crate::metrics::Snapshot;
use crate::node::NodeContext;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Debug, Serialize)]
struct Health {
    status: &'static str,
    node: String,
    uptime_secs: u64,
    keys: u64,
    used_bytes: u64,
    max_bytes: u64,
}

async fn health(State(ctx): State<Arc<NodeContext>>) -> Json<Health> {
    let (keys, used_bytes, max_bytes) = ctx.store.stats();
    Json(Health {
        status: "ok",
        node: ctx.self_address.clone(),
        uptime_secs: ctx.metrics.uptime_secs(),
        keys: keys as u64,
        used_bytes,
        max_bytes,
    })
}

async fn metrics(State(ctx): State<Arc<NodeContext>>) -> Json<Snapshot> {
    Json(ctx.metrics.snapshot(
        &ctx.self_address,
        &ctx.store,
        ctx.aof.as_deref(),
        ctx.heartbeat.as_deref(),
    ))
}

async fn cors(request: Request<Body>, next: Next) -> Response {
    let mut response = if request.method() == Method::OPTIONS {
        let mut response = Response::new(Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        response
    } else {
        next.run(request).await
    };
    let headers = response.headers_mut();
    headers.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    headers.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, OPTIONS"),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type"),
    );
    headers.insert("Access-Control-Max-Age", HeaderValue::from_static("3600"));
    response
}

pub async fn serve(listener: TcpListener, ctx: Arc<NodeContext>) -> std::io::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .with_state(ctx)
        .layer(middleware::from_fn(cors));
    axum::serve(listener, app).await
}