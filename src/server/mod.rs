use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir};
use crate::storage::TraceDb;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<TraceDb>,
}

#[derive(Deserialize)]
pub struct RangeQuery {
    start: Option<u64>,
    end: Option<u64>,
}

#[derive(Serialize)]
pub struct TraceResponse {
    entries: Vec<crate::storage::TraceEntry>,
    total: u64,
}

pub async fn get_trace(
    State(state): State<AppState>,
    Query(range): Query<RangeQuery>,
) -> Json<TraceResponse> {
    let total = state.db.count();
    let entries = match (range.start, range.end) {
        (Some(start), Some(end)) => state.db.get_range(start, end),
        _ => state.db.get_all(),
    };
    Json(TraceResponse { entries, total })
}

pub async fn get_step(
    State(state): State<AppState>,
    Path(step): Path<u64>,
) -> Result<Json<crate::storage::TraceEntry>, StatusCode> {
    state.db.get(step)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

pub fn create_router(db: Arc<TraceDb>) -> Router {
    let state = AppState { db };
    Router::new()
        .route("/api/trace", get(get_trace))
        .route("/api/trace/:step", get(get_step))
        .nest_service("/", ServeDir::new("web"))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(db: Arc<TraceDb>, port: u16) {
    let app = create_router(db);
    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("Server running on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}

