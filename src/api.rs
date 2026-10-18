// Author: Passwd Word
//! REST API + aggregate playlist for the frontend.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::config::{AddChannelReq, AppState, Channel};

#[derive(Serialize)]
struct StatusResp {
    channels: usize,
    live: usize,
    engine_root: String,
}

pub async fn status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let list = state.list();
    let live = list
        .iter()
        .filter(|c| c.status == crate::config::ChannelStatus::Live)
        .count();
    Json(StatusResp {
        channels: list.len(),
        live,
        engine_root: state.engine_root.clone(),
    })
}

pub async fn list_channels(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.list())
}

pub async fn add_channel(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddChannelReq>,
) -> Response {
    match state.add_channel(req.url.trim(), req.name) {
        Ok(ch) => (StatusCode::CREATED, Json(ch)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

pub async fn get_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.get(&id) {
        Some(ch) => Json(ch).into_response(),
        None => (StatusCode::NOT_FOUND, "channel not found").into_response(),
    }
}

pub async fn delete_channel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match state.remove(&id) {
        Some(ch) => Json(ch).into_response(),
        None => (StatusCode::NOT_FOUND, "channel not found").into_response(),
    }
}

/// Aggregate playlist: one entry per channel, for APTV subscription.
/// The host is taken from the request Host header so it works behind NAT.
pub async fn playlist_m3u(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("127.0.0.1:8080");
    let mut m3u = String::from("#EXTM3U\n");
    for ch in state.list() {
        let logo = "";
        m3u.push_str(&format!(
            "#EXTINF:-1 tvg-id=\"{}\" tvg-logo=\"{}\" group-title=\"P2P\",{}\nhttp://{}/live/{}.m3u8\n",
            ch.id, logo, ch.name, host, ch.id
        ));
    }
    (
        [(header::CONTENT_TYPE, "audio/x-mpegurl")],
        m3u,
    )
        .into_response()
}
