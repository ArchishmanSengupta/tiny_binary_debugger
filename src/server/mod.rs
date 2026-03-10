use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use crate::storage::TraceDb;
use crate::stats::TraceStats;

// Embed the web viewer directly in the binary so it works from any directory.
const INDEX_HTML: &str = include_str!("../../web/index.html");

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<TraceDb>,
}

#[derive(Deserialize)]
pub struct RangeQuery {
    start: Option<u64>,
    end: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct TraceResponse {
    entries: Vec<crate::storage::TraceEntry>,
    total: u64,
}

#[derive(Serialize, Deserialize)]
pub struct CountResponse {
    count: u64,
}

async fn index_handler() -> Html<&'static str> {
    Html(INDEX_HTML)
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

pub async fn get_count(
    State(state): State<AppState>,
) -> Json<CountResponse> {
    Json(CountResponse { count: state.db.count() })
}

pub async fn get_stats(
    State(state): State<AppState>,
) -> Json<TraceStats> {
    Json(TraceStats::analyze(&state.db))
}

pub fn create_router(db: Arc<TraceDb>) -> Router {
    let state = AppState { db };
    Router::new()
        .route("/", get(index_handler))
        .route("/api/trace", get(get_trace))
        .route("/api/trace/count", get(get_count))
        .route("/api/trace/:step", get(get_step))
        .route("/api/stats", get(get_stats))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn serve(db: Arc<TraceDb>, port: u16) {
    let app = create_router(db);
    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("\n  Server running at http://{}", addr);
    println!("  Press Ctrl+C to stop\n");
    axum::serve(listener, app).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MemChange, TraceEntry};
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn make_entry(step: u64, pc: u64, insn: &str) -> TraceEntry {
        TraceEntry {
            step,
            pc,
            insn_bytes: vec![0xAA],
            insn_text: insn.to_string(),
            regs: serde_json::json!({"x0": step, "sp": 0x7000}).to_string(),
            mem_changes: vec![],
        }
    }

    fn make_entry_with_mem(step: u64, insn: &str, n: usize) -> TraceEntry {
        let changes: Vec<MemChange> = (0..n)
            .map(|i| MemChange {
                addr: 0x1000 + i as u64,
                old_val: 0,
                new_val: i as u8,
            })
            .collect();
        TraceEntry {
            step,
            pc: 0x1000 + step * 4,
            insn_bytes: vec![0xBB],
            insn_text: insn.to_string(),
            regs: serde_json::json!({"x0": 42}).to_string(),
            mem_changes: changes,
        }
    }

    fn test_db(entries: Vec<TraceEntry>) -> Arc<TraceDb> {
        let db = TraceDb::new(":memory:").unwrap();
        for e in entries {
            db.insert(e).unwrap();
        }
        Arc::new(db)
    }

    fn request(method: &str, uri: &str) -> axum::http::Request<Body> {
        axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    // ── GET / ──

    #[tokio::test]
    async fn index_returns_html() {
        let app = create_router(test_db(vec![]));
        let resp = app.oneshot(request("GET", "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("text/html"),
            "Content-Type should be text/html, got: {}",
            ct
        );

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&body);
        assert!(
            html.contains("<html") || html.contains("<!DOCTYPE"),
            "Response should contain HTML"
        );
    }

    // ── GET /api/trace ──

    #[tokio::test]
    async fn trace_returns_all_entries() {
        let entries = vec![
            make_entry(0, 0x1000, "nop"),
            make_entry(1, 0x1004, "mov x0, #1"),
            make_entry(2, 0x1008, "ret"),
        ];
        let app = create_router(test_db(entries));
        let resp = app
            .oneshot(request("GET", "/api/trace"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: TraceResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total, 3);
        assert_eq!(json.entries.len(), 3);
    }

    #[tokio::test]
    async fn trace_empty_db() {
        let app = create_router(test_db(vec![]));
        let resp = app
            .oneshot(request("GET", "/api/trace"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: TraceResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.total, 0);
        assert!(json.entries.is_empty());
    }

    #[tokio::test]
    async fn trace_range_query() {
        let entries: Vec<TraceEntry> = (0..10)
            .map(|i| make_entry(i, 0x1000 + i * 4, "nop"))
            .collect();
        let app = create_router(test_db(entries));
        let resp = app
            .oneshot(request("GET", "/api/trace?start=3&end=7"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: TraceResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.entries.len(), 5);
        assert_eq!(json.entries[0].step, 3);
        assert_eq!(json.entries[4].step, 7);
        assert_eq!(json.total, 10); // total is always the full count
    }

    // ── GET /api/trace/{step} ──

    #[tokio::test]
    async fn get_step_existing() {
        let entries = vec![make_entry(42, 0xCAFE, "add x0, x1, x2")];
        let app = create_router(test_db(entries));
        let resp = app
            .oneshot(request("GET", "/api/trace/42"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let entry: TraceEntry = serde_json::from_slice(&body).unwrap();
        assert_eq!(entry.step, 42);
        assert_eq!(entry.pc, 0xCAFE);
        assert_eq!(entry.insn_text, "add x0, x1, x2");
    }

    #[tokio::test]
    async fn get_step_not_found() {
        let app = create_router(test_db(vec![make_entry(0, 0x1000, "nop")]));
        let resp = app
            .oneshot(request("GET", "/api/trace/999"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── GET /api/trace/count ──

    #[tokio::test]
    async fn count_endpoint() {
        let entries: Vec<TraceEntry> = (0..5)
            .map(|i| make_entry(i, 0x1000 + i * 4, "nop"))
            .collect();
        let app = create_router(test_db(entries));
        let resp = app
            .oneshot(request("GET", "/api/trace/count"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: CountResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.count, 5);
    }

    #[tokio::test]
    async fn count_empty_db() {
        let app = create_router(test_db(vec![]));
        let resp = app
            .oneshot(request("GET", "/api/trace/count"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: CountResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.count, 0);
    }

    // ── GET /api/stats ──

    #[tokio::test]
    async fn stats_endpoint() {
        let entries = vec![
            make_entry(0, 0x1000, "bl #0x2000 ; CALL [depth:1]"),
            make_entry(1, 0x2000, "mov x0, #0"),
            make_entry_with_mem(2, "str x0, [sp]", 3),
            make_entry(3, 0x2008, "ret ; RETURN [depth:0]"),
        ];
        let app = create_router(test_db(entries));
        let resp = app
            .oneshot(request("GET", "/api/stats"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let stats: TraceStats = serde_json::from_slice(&body).unwrap();
        assert_eq!(stats.total_steps, 4);
        assert_eq!(stats.call_count, 1);
        assert_eq!(stats.ret_count, 1);
        assert_eq!(stats.mem_change_count, 3);
    }

    // ── 404 for unknown routes ──

    #[tokio::test]
    async fn unknown_route_returns_404() {
        let app = create_router(test_db(vec![]));
        let resp = app
            .oneshot(request("GET", "/api/nonexistent"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── CORS headers present ──

    #[tokio::test]
    async fn cors_headers_present() {
        let app = create_router(test_db(vec![]));
        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/api/trace/count")
            .header("Origin", "http://localhost:3000")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // CorsLayer::permissive() should set access-control-allow-origin
        assert!(
            resp.headers().contains_key("access-control-allow-origin"),
            "CORS header should be present"
        );
    }
}
