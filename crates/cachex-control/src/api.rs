

use std::convert::Infallible;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;

use crate::experiments::eviction::{self, EvictionParams};
use crate::experiments::failure::{self, FailureParams};
use crate::experiments::hashing::{self, HashingParams};
use crate::experiments::replication::{self, ReplicationParams};
use crate::experiments::scalability::{self, ScalabilityParams};
use crate::experiments::ttl::{self, TtlParams};
use crate::experiments::{run_stub, StubParams};
use crate::load::{self, LoadParams};
use crate::nodes;
use crate::state::{AppState, ClusterStatus, NodeInfo};

#[derive(Debug, Serialize)]
struct Health {
    ok: bool,
    root: String,
    server_ready: bool,
    control_dir: String,
}

async fn health(State(state): State<Arc<AppState>>) -> Json<Health> {
    Json(Health {
        ok: true,
        root: state.root.to_string_lossy().into_owned(),
        server_ready: state.server_exe.exists(),
        control_dir: state.control_dir.to_string_lossy().into_owned(),
    })
}

async fn cluster_status(state: &Arc<AppState>) -> ClusterStatus {
    let spec = state.spec.lock().unwrap().clone();
    let pids = state.pids.lock().unwrap();
    let mut nodes_info = Vec::new();
    for node_id in 1..=spec.node_count {
        let pid = pids.get(&spec.public_port(node_id)).copied();
        nodes_info.push(NodeInfo {
            id: node_id,
            public_port: spec.public_port(node_id),
            metrics_port: spec.metrics_port(node_id),
            address: spec.public_address(node_id),
            pid,
        });
    }
    ClusterStatus {
        spec,
        nodes: nodes_info,
        ready: state.server_exe.exists(),
    }
}

async fn get_cluster_status(State(state): State<Arc<AppState>>) -> Json<ClusterStatus> {
    Json(cluster_status(&state).await)
}

#[derive(Debug, Deserialize, Default)]
struct StartBody {
    
    #[serde(default)]
    nodes: Option<u16>,
}

async fn start_cluster(
    State(state): State<Arc<AppState>>,
    body: Option<Json<StartBody>>,
) -> Result<Json<ClusterStatus>, (StatusCode, String)> {
    if let Some(Json(body)) = body {
        if let Some(nodes) = body.nodes {
            if nodes < 1 || nodes > 16 {
                return Err((StatusCode::BAD_REQUEST, "nodes must be 1..=16".into()));
            }
            state.spec.lock().unwrap().node_count = nodes;
        }
    }
    if !state.server_exe.exists() {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("server binary not found at {}", state.server_exe.display()),
        ));
    }
    nodes::start_cluster(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(cluster_status(&state).await))
}

async fn stop_cluster(State(state): State<Arc<AppState>>) -> Result<Json<ClusterStatus>, (StatusCode, String)> {
    nodes::stop_cluster(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(cluster_status(&state).await))
}

async fn kill_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<u16>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let spec = state.spec.lock().unwrap().clone();
    if node_id < 1 || node_id > spec.node_count {
        return Err((StatusCode::NOT_FOUND, format!("no node {node_id}")));
    }
    match nodes::kill_node(&state, node_id).await {
        Ok(pid) => Ok(Json(serde_json::json!({ "node": node_id, "pid": pid }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn restart_node(
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<u16>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let spec = state.spec.lock().unwrap().clone();
    if node_id < 1 || node_id > spec.node_count {
        return Err((StatusCode::NOT_FOUND, format!("no node {node_id}")));
    }
    match nodes::restart_node(&state, node_id).await {
        Ok(()) => Ok(Json(serde_json::json!({ "node": node_id, "restarted": true }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

#[derive(Debug, Deserialize)]
struct ReplicationBody {
    factor: u32,
}

async fn set_replication(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ReplicationBody>,
) -> Result<Json<ClusterStatus>, (StatusCode, String)> {
    if body.factor < 1 || body.factor > 4 {
        return Err((StatusCode::BAD_REQUEST, "factor must be 1..=4".into()));
    }
    let unchanged = {
        let mut spec = state.spec.lock().unwrap();
        if spec.replication_factor == body.factor {
            true
        } else {
            spec.replication_factor = body.factor;
            false
        }
    };
    if unchanged {
        return Ok(Json(cluster_status(&state).await));
    }
    
    
    let _ = nodes::stop_cluster(&state).await;
    nodes::start_cluster(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(cluster_status(&state).await))
}

#[derive(Debug, Deserialize)]
struct ScaleBody {
    target: u16,
}

async fn scale_cluster(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ScaleBody>,
) -> Result<Json<ClusterStatus>, (StatusCode, String)> {
    if body.target < 1 || body.target > 16 {
        return Err((StatusCode::BAD_REQUEST, "target must be 1..=16".into()));
    }
    let unchanged = {
        let mut spec = state.spec.lock().unwrap();
        if spec.node_count == body.target {
            true
        } else {
            spec.node_count = body.target;
            false
        }
    };
    if unchanged {
        return Ok(Json(cluster_status(&state).await));
    }
    let _ = nodes::stop_cluster(&state).await;
    nodes::start_cluster(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(cluster_status(&state).await))
}

async fn start_load(
    State(state): State<Arc<AppState>>,
    Json(params): Json<LoadParams>,
) -> Json<serde_json::Value> {
    let id = load::start_load(&state, params);
    Json(serde_json::json!({ "id": id }))
}

async fn get_load(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match load::load_status(&state, &id) {
        Some(status) => Json(serde_json::json!({
            "id": status.id,
            "clients": status.clients,
            "requests": status.requests,
            "get_ratio": status.get_ratio,
            "keys": status.keys,
            "value_size": status.value_size,
            "done": status.done,
            "error": status.error,
            "report": status.report,
        })),
        None => Json(serde_json::json!({ "error": format!("no load `{id}`") })),
    }
}

async fn run_hashing_experiment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<HashingParams>,
) -> Json<serde_json::Value> {
    Json(hashing::run_hashing(&state, params).await)
}

async fn run_eviction_experiment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<EvictionParams>,
) -> Json<serde_json::Value> {
    Json(eviction::run_eviction(&state, params).await)
}

async fn run_ttl_experiment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<TtlParams>,
) -> Json<serde_json::Value> {
    Json(ttl::run_ttl(&state, params).await)
}

async fn run_replication_experiment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ReplicationParams>,
) -> Json<serde_json::Value> {
    Json(replication::run_replication(&state, params).await)
}

async fn run_scalability_experiment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<ScalabilityParams>,
) -> Json<serde_json::Value> {
    Json(scalability::run_scalability(&state, params).await)
}

async fn run_failure_experiment(
    State(state): State<Arc<AppState>>,
    Json(params): Json<FailureParams>,
) -> Json<serde_json::Value> {
    Json(failure::run_failure(&state, params).await)
}

async fn run_stub_experiment(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(params): Json<StubParams>,
) -> Json<serde_json::Value> {
    let _ = params;
    Json(run_stub(&state, &name).await)
}

async fn latest_result(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let last = state.last_experiment.lock().unwrap().clone();
    Json(last.unwrap_or_else(|| serde_json::json!({ "status": "none" })))
}

async fn events(State(state): State<Arc<AppState>>) -> Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(event) => {
                let payload = serde_json::to_string(&event).unwrap_or_else(|_| "{}".into());
                Some(Ok(SseEvent::default().data(payload)))
            }
            Err(_) => None,
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
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
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type"),
    );
    headers.insert("Access-Control-Max-Age", HeaderValue::from_static("3600"));
    response
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/control/health", get(health))
        .route("/control/cluster/status", get(get_cluster_status))
        .route("/control/cluster/start", post(start_cluster))
        .route("/control/cluster/stop", post(stop_cluster))
        .route("/control/cluster/node/{node_id}/kill", post(kill_node))
        .route("/control/cluster/node/{node_id}/restart", post(restart_node))
        .route("/control/cluster/replication", post(set_replication))
        .route("/control/cluster/scale", post(scale_cluster))
        .route("/control/load/start", post(start_load))
        .route("/control/load/{id}", get(get_load))
        .route("/control/experiment/hashing", post(run_hashing_experiment))
        .route("/control/experiment/eviction", post(run_eviction_experiment))
        .route("/control/experiment/ttl", post(run_ttl_experiment))
        .route("/control/experiment/replication", post(run_replication_experiment))
        .route("/control/experiment/scalability", post(run_scalability_experiment))
        .route("/control/experiment/failure", post(run_failure_experiment))
        .route(
            "/control/experiment/{name}",
            post(run_stub_experiment),
        )
        .route("/control/results/latest", get(latest_result))
        .route("/control/events", get(events))
        .with_state(state)
        .layer(middleware::from_fn(cors))
}