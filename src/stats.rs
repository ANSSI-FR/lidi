//! Lightweight runtime observability state shared between worker threads and the
//! optional observability HTTP server.
//!
//! Hot-path counters are plain `AtomicU64`/`AtomicUsize` updated with
//! `Ordering::Relaxed`: we want monotonic increments, not cross-counter ordering.
//! Per-client entries live under a `Mutex<Vec<...>>` that is only touched on
//! client connect / finish / abort (the same moments the existing code already
//! does a log write) and on a snapshot read from the HTTP layer.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use crate::protocol;

pub const ROLE_SEND: &str = "send";
pub const ROLE_RECEIVE: &str = "receive";

/// Lifecycle state of a transfer, as surfaced to the UI.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Connected,
    Finished,
    Aborted,
}

impl ClientState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connected => "Connected",
            Self::Finished => "Finished",
            Self::Aborted => "Aborted",
        }
    }
}

/// One row in the clients table rendered by the UI.
#[derive(Clone)]
pub struct ClientEntry {
    pub id: protocol::ClientId,
    pub started_unix_ms: u64,
    pub bytes: u64,
    pub state: ClientState,
}

/// An owned, point-in-time snapshot consumed by the HTTP layer so the lock
/// doesn't cross any serialization work.
pub struct StatsSnapshot {
    pub uptime_secs: u64,
    pub now_unix_ms: u64,
    pub bytes_total: u64,
    pub packets_total: u64,
    pub transfers_started: u64,
    pub transfers_finished: u64,
    pub transfers_aborted: u64,
    pub active_count: usize,
    pub last_heartbeat_unix_ms: u64,
    pub clients: Vec<ClientEntry>,
}

pub struct Stats {
    pub role: &'static str,
    started_at: Instant,
    bytes_total: AtomicU64,
    packets_total: AtomicU64,
    transfers_started: AtomicU64,
    transfers_finished: AtomicU64,
    transfers_aborted: AtomicU64,
    active_count: AtomicUsize,
    last_heartbeat_unix_ms: AtomicU64,
    clients: Mutex<Vec<ClientEntry>>,
    clients_cap: usize,
}

impl Stats {
    /// `clients_cap` bounds the number of historical entries retained; on
    /// overflow the oldest non-active entry is dropped so active transfers are
    /// always visible.
    #[must_use]
    pub fn new(role: &'static str, clients_cap: usize) -> Self {
        Self {
            role,
            started_at: Instant::now(),
            bytes_total: AtomicU64::new(0),
            packets_total: AtomicU64::new(0),
            transfers_started: AtomicU64::new(0),
            transfers_finished: AtomicU64::new(0),
            transfers_aborted: AtomicU64::new(0),
            active_count: AtomicUsize::new(0),
            last_heartbeat_unix_ms: AtomicU64::new(0),
            clients: Mutex::new(Vec::new()),
            clients_cap: clients_cap.max(8),
        }
    }

    pub fn client_connected(&self, id: protocol::ClientId) {
        self.transfers_started.fetch_add(1, Ordering::Relaxed);
        self.active_count.fetch_add(1, Ordering::Relaxed);
        let entry = ClientEntry {
            id,
            started_unix_ms: unix_now_ms(),
            bytes: 0,
            state: ClientState::Connected,
        };
        if let Ok(mut guard) = self.clients.lock() {
            while guard.len() >= self.clients_cap {
                // Drop the oldest finished/aborted entry; if none, drop the
                // oldest (shouldn't normally happen since active_count <=
                // max_clients << clients_cap).
                let idx = guard
                    .iter()
                    .position(|e| e.state != ClientState::Connected)
                    .unwrap_or(0);
                guard.remove(idx);
            }
            guard.push(entry);
        }
    }

    pub fn add_bytes(&self, id: protocol::ClientId, bytes: u64) {
        self.bytes_total.fetch_add(bytes, Ordering::Relaxed);
        if let Ok(mut guard) = self.clients.lock() {
            for entry in guard.iter_mut().rev() {
                if entry.id == id && entry.state == ClientState::Connected {
                    entry.bytes = entry.bytes.saturating_add(bytes);
                    break;
                }
            }
        }
    }

    pub fn client_finished(&self, id: protocol::ClientId) {
        self.transfers_finished.fetch_add(1, Ordering::Relaxed);
        self.dec_active();
        self.mark_state(id, ClientState::Finished);
    }

    pub fn client_aborted(&self, id: protocol::ClientId) {
        self.transfers_aborted.fetch_add(1, Ordering::Relaxed);
        self.dec_active();
        self.mark_state(id, ClientState::Aborted);
    }

    pub fn add_packets(&self, n: u64) {
        self.packets_total.fetch_add(n, Ordering::Relaxed);
    }

    pub fn heartbeat_seen(&self) {
        self.last_heartbeat_unix_ms
            .store(unix_now_ms(), Ordering::Relaxed);
    }

    fn dec_active(&self) {
        let _ = self.active_count.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |v| Some(v.saturating_sub(1)),
        );
    }

    fn mark_state(&self, id: protocol::ClientId, state: ClientState) {
        if let Ok(mut guard) = self.clients.lock() {
            for entry in guard.iter_mut().rev() {
                if entry.id == id && entry.state == ClientState::Connected {
                    entry.state = state;
                    break;
                }
            }
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> StatsSnapshot {
        let clients = self
            .clients
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        StatsSnapshot {
            uptime_secs: self.started_at.elapsed().as_secs(),
            now_unix_ms: unix_now_ms(),
            bytes_total: self.bytes_total.load(Ordering::Relaxed),
            packets_total: self.packets_total.load(Ordering::Relaxed),
            transfers_started: self.transfers_started.load(Ordering::Relaxed),
            transfers_finished: self.transfers_finished.load(Ordering::Relaxed),
            transfers_aborted: self.transfers_aborted.load(Ordering::Relaxed),
            active_count: self.active_count.load(Ordering::Relaxed),
            last_heartbeat_unix_ms: self.last_heartbeat_unix_ms.load(Ordering::Relaxed),
            clients,
        }
    }
}

pub(crate) fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}
