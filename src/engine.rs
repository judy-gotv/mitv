// Author: Passwd Word
//! Engine sidecar manager.
//!
//! Each channel gets one sidecar:
//!   qemu-arm-static -L <root> <root>/launcher <root>/libp2p_patched.so \
//!       -s <server> -c <guid> -o <port>
//! The engine then serves the stream over HTTP on 127.0.0.1:<port>:
//!   http://127.0.0.1:<port>/forcelive&begin=loading&count=0/<guid>.ts
//!
//! Layout of ENGINE_ROOT:
//!   ENGINE_ROOT/
//!     qemu-arm-static
//!     launcher            (ARM launcher binary, patchelf'd with DT_NEEDED libbionic_shim)
//!     libp2p_patched.so  (engine with forcetv_system_test/std_test patched to no-op)
//!     lib/                (ARM glibc renamed to bionic SONAMEs + libbionic_shim.so)

use std::collections::HashSet;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::sleep;

use crate::config::{AppState, ChannelStatus};

/// Track which channels already have a sidecar running.
async fn spawn_sidecar(state: &Arc<AppState>, id: &str) {
    let ch = match state.get(id) {
        Some(c) => c,
        None => return,
    };
    let qemu = std::env::var("QEMU_ARM_STATIC").unwrap_or_else(|_| "qemu-arm-static".to_string());
    let launcher = format!("{}/launcher", state.engine_root);
    let so = format!("{}/libp2p_patched.so", state.engine_root);

    tracing::info!(
        "starting sidecar for {} ({} -> 127.0.0.1:{})",
        ch.name,
        ch.url,
        ch.engine_port
    );

    let mut cmd = Command::new(&qemu);
    cmd.arg("-L")
        .arg(&state.engine_root)
        .arg(&launcher)
        .arg(&so)
        .arg("-s")
        .arg(&ch.server)
        .arg("-c")
        .arg(&ch.guid)
        .arg("-o")
        .arg(ch.engine_port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    match cmd.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            tracing::info!("sidecar for {} spawned, pid {:?}", ch.name, pid);
            // Reap in background; supervisor handles health.
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
        }
        Err(e) => {
            tracing::error!("failed to spawn sidecar for {}: {}", ch.name, e);
            state.set_status(
                id,
                ChannelStatus::Error,
                format!("spawn failed: {} (is ENGINE_ROOT={} populated?)", e, state.engine_root),
            );
        }
    }
}

/// Probe the engine's local HTTP port. The engine answers
/// `/forcelive&begin=loading&count=0/<guid>.ts` with a TS stream.
async fn probe(ch: &crate::config::Channel) -> bool {
    let url = format!(
        "http://127.0.0.1:{}/forcelive&begin=loading&count=0/{}.ts",
        ch.engine_port, ch.guid
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    // A HEAD is enough to know the port is up; the engine supports range/HEAD.
    match client.head(&url).send().await {
        Ok(r) => r.status().is_success() || r.status().as_u16() == 206,
        Err(_) => false,
    }
}

/// Supervisor: every 10s, ensure each channel has a sidecar and update status.
pub async fn supervisor(state: Arc<AppState>) {
    let mut started: HashSet<String> = HashSet::new();
    loop {
        let channels = state.list();
        for ch in channels {
            if !started.contains(&ch.id) {
                spawn_sidecar(&state, &ch.id).await;
                started.insert(ch.id.clone());
                continue;
            }
            // Health check (skip if already errored to avoid log spam; retry every loop anyway).
            if probe(&ch).await {
                state.set_status(
                    &ch.id,
                    ChannelStatus::Live,
                    format!("serving on 127.0.0.1:{}", ch.engine_port),
                );
            } else {
                // Only downgrade Live -> Error; keep Starting until first success or timeout.
                let cur = state.get(&ch.id).map(|c| c.status);
                if cur == Some(ChannelStatus::Live) {
                    state.set_status(
                        &ch.id,
                        ChannelStatus::Error,
                        "engine port unreachable, sidecar may have crashed".to_string(),
                    );
                    started.remove(&ch.id); // allow respawn next loop
                }
            }
        }
        // Forget removed channels.
        let ids: HashSet<String> = state.list().iter().map(|c| c.id.clone()).collect();
        started.retain(|id| ids.contains(id));
        sleep(Duration::from_secs(10)).await;
    }
}
