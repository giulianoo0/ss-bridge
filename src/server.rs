use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};

use crate::engine::Engine;
use crate::portmap;

pub const PORT: u16 = 32227;

pub async fn serve(engine: Arc<Engine>) -> anyhow::Result<()> {
    // allow_private_network makes preflights answer Chrome's private-network
    // check, without which a public page (ss.giuli.dev) is blocked from the
    // preflighted POSTs even after the user has granted local network access.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_private_network(true)
        .expose_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/add", post(add))
        .route("/select", post(select))
        .route("/stats/:id", get(stats))
        .route("/stream/:id/:index", get(stream))
        .route("/close", post(close))
        .layer(cors)
        .with_state(engine);

    let listener = TcpListener::bind(("127.0.0.1", PORT)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Serialize)]
struct Health {
    name: &'static str,
    version: &'static str,
    // "open" | "no-mapping" | "no-router" | "unknown" — a closed port is the
    // difference between a torrent that flies and one that crawls, and the
    // page is where the host will actually read the warning.
    #[serde(rename = "portMapping")]
    port_mapping: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health {
        name: "ss-bridge",
        version: env!("CARGO_PKG_VERSION"),
        port_mapping: portmap::state().as_str(),
    })
}

#[derive(Deserialize)]
struct AddReq {
    magnet: String,
}

async fn add(State(engine): State<Arc<Engine>>, Json(req): Json<AddReq>) -> Result<Response, ApiError> {
    let added = engine.add(&req.magnet).await?;
    Ok(Json(added).into_response())
}

#[derive(Deserialize)]
struct SelectReq {
    id: String,
    #[serde(rename = "fileIndex")]
    file_index: usize,
}

async fn select(State(engine): State<Arc<Engine>>, Json(req): Json<SelectReq>) -> Result<Response, ApiError> {
    engine.select(&req.id, req.file_index).await?;
    Ok(StatusCode::OK.into_response())
}

async fn stats(State(engine): State<Arc<Engine>>, Path(id): Path<String>) -> Result<Response, ApiError> {
    Ok(Json(engine.stats(&id)?).into_response())
}

#[derive(Deserialize)]
struct CloseReq {
    id: String,
}

async fn close(State(engine): State<Arc<Engine>>, Json(req): Json<CloseReq>) -> Result<Response, ApiError> {
    engine.close(&req.id).await;
    Ok(StatusCode::OK.into_response())
}

async fn stream(
    State(engine): State<Arc<Engine>>,
    Path((id, index)): Path<(String, usize)>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let size = engine.file_size(&id, index)?;
    let (start, end) = parse_range(headers.get(header::RANGE), size);
    let len = end - start + 1;
    let bytes = engine.read_range(&id, index, start, len).await?;

    let mut res = Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"))
        .header(header::CONTENT_LENGTH, len.to_string());
    // Reflect the origin so a preflighted ranged fetch is not blocked.
    res = res.header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    Ok(res.body(Body::from(bytes)).unwrap())
}

// The whole range is buffered before the response leaves; an open-ended
// range would mean the rest of the file in RAM, so it is served in a
// 64MB-capped slice and the client follows up with the next Range.
const MAX_RANGE: u64 = 64 * 1024 * 1024;

fn parse_range(value: Option<&header::HeaderValue>, size: u64) -> (u64, u64) {
    let last = size.saturating_sub(1);
    let raw = match value.and_then(|v| v.to_str().ok()) {
        Some(v) => v,
        None => return (0, last.min(MAX_RANGE - 1)),
    };
    let spec = raw.strip_prefix("bytes=").unwrap_or(raw);
    let mut parts = spec.splitn(2, '-');
    let start = parts.next().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    let end = match parts.next().and_then(|s| s.trim().parse::<u64>().ok()) {
        Some(end) => end.min(last),
        None => last.min(start + MAX_RANGE - 1),
    };
    if end < start {
        (0, last.min(MAX_RANGE - 1))
    } else {
        (start, end)
    }
}

struct ApiError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(err: E) -> Self {
        ApiError(err.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::BAD_GATEWAY, self.0.to_string()).into_response()
    }
}
