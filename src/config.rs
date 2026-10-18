// Author: Passwd Word
//! Shared application state: channel registry and engine sidecar bookkeeping.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Parse `p2p://<server>:<port>/<32-hex-guid>` (also accepts `mitv://`).
/// Returns (server, guid).
pub fn parse_p2p_url(url: &str) -> Result<(String, String), String> {
    let rest = url
        .strip_prefix("p2p://")
        .or_else(|| url.strip_prefix("mitv://"))
        .or_else(|| url.strip_prefix("P2P://"))
        .ok_or_else(|| "URL must start with p2p:// or mitv://".to_string())?;
    let (server, guid) = rest
        .rsplit_once('/')
        .ok_or_else(|| "URL must look like p2p://server:port/<32-hex-guid>".to_string())?;
    if guid.len() != 32 || !guid.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("channel id must be 32 hex chars".to_string());
    }
    if server.is_empty() {
        return Err("missing server part".to_string());
    }
    Ok((server.to_string(), guid.to_lowercase()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    /// Engine sidecar starting / handshaking.
    Starting,
    /// Engine HTTP port is up and serving data.
    Live,
    /// Engine process died or health check failed.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub url: String,
    pub server: String,
    pub guid: String,
    /// Local port assigned to this channel's engine sidecar.
    pub engine_port: u16,
    pub status: ChannelStatus,
    pub detail: String,
    pub added_at: u64,
}

#[derive(Debug, Deserialize)]
pub struct AddChannelReq {
    pub url: String,
    pub name: Option<String>,
}

pub struct AppState {
    pub engine_root: String,
    pub inner: Mutex<HashMap<String, Channel>>,
    /// Next engine port to assign (9906, 9907, ...).
    pub next_port: Mutex<u16>,
}

impl AppState {
    pub fn new(engine_root: String) -> Self {
        Self {
            engine_root,
            inner: Mutex::new(HashMap::new()),
            next_port: Mutex::new(9906),
        }
    }

    pub fn add_channel(&self, url: &str, name: Option<String>) -> Result<Channel, String> {
        let (server, guid) = parse_p2p_url(url)?;
        let mut ports = self.next_port.lock().unwrap();
        let engine_port = *ports;
        *ports = ports.wrapping_add(1);
        drop(ports);

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ch = Channel {
            id: Uuid::new_v4().to_string()[..8].to_string(),
            name: name.unwrap_or_else(|| format!("频道 {}", guid[..8].to_string())),
            url: url.to_string(),
            server,
            guid,
            engine_port,
            status: ChannelStatus::Starting,
            detail: "engine sidecar starting".to_string(),
            added_at: now,
        };
        self.inner.lock().unwrap().insert(ch.id.clone(), ch.clone());
        Ok(ch)
    }

    pub fn list(&self) -> Vec<Channel> {
        let mut v: Vec<Channel> = self.inner.lock().unwrap().values().cloned().collect();
        v.sort_by_key(|c| c.added_at);
        v
    }

    pub fn get(&self, id: &str) -> Option<Channel> {
        self.inner.lock().unwrap().get(id).cloned()
    }

    pub fn remove(&self, id: &str) -> Option<Channel> {
        self.inner.lock().unwrap().remove(id)
    }

    pub fn set_status(&self, id: &str, status: ChannelStatus, detail: String) {
        if let Some(ch) = self.inner.lock().unwrap().get_mut(id) {
            ch.status = status;
            ch.detail = detail;
        }
    }
}
