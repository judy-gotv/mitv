// Author: Passwd Word
// p2p-http-gateway: P2P (NagaSoft VJLive / ForceTV) to HTTP/HLS gateway.
// The gateway manages one engine sidecar (qemu-arm + libforcetv.so) per
// channel, and proxies the engine's local HTTP output as HLS.
//
// URL format: p2p://<server-ip>:19806/<32-hex-channel-guid>
// Engine CLI:  -s <server> -c <guid> -o <local-port>
// Engine HTTP: http://127.0.0.1:<port>/forcelive&begin=loading&count=0/<guid>.ts
//
// NOTE (2026-10-18): the engine's forcetv_init currently crashes (NULL+8)
// under qemu-user + bionic shim in this sandbox. The gateway structure,
// API, frontend and proxy plumbing are complete; once the engine sidecar
// runs, streaming works end-to-end. See README.md and
// ~/workspace/p2p-protocol-recon/EMULATION.md for the RE background.

mod api;
mod config;
mod engine;
mod proxy;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::get, Router};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "p2p_http_gateway=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let bind: SocketAddr = std::env::var("BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()
        .expect("invalid BIND");
    let engine_root = std::env::var("ENGINE_ROOT")
        .unwrap_or_else(|_| "/opt/p2p-engine".to_string());

    let state = Arc::new(AppState::new(engine_root));

    // Spawn the engine health supervisor.
    {
        let st = state.clone();
        tokio::spawn(async move { engine::supervisor(st).await });
    }

    let app = Router::new()
        .route("/api/channels", get(api::list_channels).post(api::add_channel))
        .route(
            "/api/channels/:id",
            get(api::get_channel).delete(api::delete_channel),
        )
        .route("/api/status", get(api::status))
        .route("/playlist.m3u", get(api::playlist_m3u))
        .route("/live/:id.m3u8", get(proxy::hls_playlist))
        .route("/live/:id/:seg", get(proxy::segment))
        .nest_service("/", ServeDir::new("frontend"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    tracing::info!("p2p-http-gateway listening on {}", bind);
    tracing::info!("frontend: http://{}/", bind);
    let listener = tokio::net::TcpListener::bind(bind).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
