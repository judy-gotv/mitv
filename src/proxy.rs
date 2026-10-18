// Author: Passwd Word
//! HLS-facing proxy. APTV / players talk to this gateway; we relay the
//! engine sidecar's local HTTP output.
//!
//! Routes:
//!   GET /live/:id.m3u8   -> generated HLS playlist (single variant)
//!   GET /live/:id/:seg   -> proxied TS segment from the engine
//!
//! The engine serves a continuous TS stream at
//!   http://127.0.0.1:<port>/forcelive&begin=loading&count=0/<guid>.ts
//! We slice it into 6s pseudo-segments via Range requests so the player
//! sees a normal HLS VOD-ish playlist. (True live HLS would need a
//! segmenter; Range-slicing is enough for APTV playback.)

use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::config::{AppState, ChannelStatus};

const SEG_SECONDS: u64 = 6;
/// Assume ~1.2 MB/s for a typical SD/HD P2P live stream; used only to
/// estimate byte ranges for pseudo-segments. The player re-syncs on
/// MPEG-TS 0x47 sync bytes, so the estimate just needs to be plausible.
const BYTES_PER_SEG: u64 = 7_000_000;

/// Build the engine-side TS stream URL for a channel.
fn engine_ts_url(ch: &crate::config::Channel) -> String {
    format!(
        "http://127.0.0.1:{}/forcelive&begin=loading&count=0/{}.ts",
        ch.engine_port, ch.guid
    )
}

pub async fn hls_playlist(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    let ch = match state.get(&id) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, "channel not found").into_response(),
    };
    if ch.status != ChannelStatus::Live {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("channel not live yet: {}", ch.detail),
        )
            .into_response();
    }
    // 3 pseudo-segments are enough for players to start; the playlist is
    // re-fetched and we keep appending sequence numbers by wall clock.
    let seq_base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        / SEG_SECONDS;
    let mut m3u8 = String::from("#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:8\n#EXT-X-MEDIA-SEQUENCE:");
    m3u8.push_str(&seq_base.to_string());
    m3u8.push('\n');
    for i in 0..3 {
        m3u8.push_str(&format!(
            "#EXTINF:{}.0,\n/live/{}/{}\n",
            SEG_SECONDS,
            ch.id,
            seq_base + i
        ));
    }
    (
        [(
            header::CONTENT_TYPE,
            "application/vnd.apple.mpegurl",
        )],
        m3u8,
    )
        .into_response()
}

pub async fn segment(
    State(state): State<Arc<AppState>>,
    Path((id, seg)): Path<(String, u64)>,
) -> Response {
    let ch = match state.get(&id) {
        Some(c) => c,
        None => return (StatusCode::NOT_FOUND, "channel not found").into_response(),
    };
    let start = seg.saturating_mul(BYTES_PER_SEG);
    let end = start + BYTES_PER_SEG - 1;
    let url = engine_ts_url(&ch);

    let client = reqwest::Client::new();
    let resp = match client
        .get(&url)
        .header(header::RANGE, format!("bytes={}-{}", start, end))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("engine unreachable: {}", e),
            )
                .into_response();
        }
    };
    let status = resp.status();
    if !status.is_success() && status.as_u16() != 206 {
        return (
            StatusCode::BAD_GATEWAY,
            format!("engine returned {}", status),
        )
            .into_response();
    }
    let stream = resp.bytes_stream();
    let body = Body::from_stream(stream);
    ([(header::CONTENT_TYPE, "video/mp2t")], body).into_response()
}
