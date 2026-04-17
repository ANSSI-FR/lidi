//! In-memory bounded log buffer used by the observability HTTP server.
//!
//! A [`TeeLogger`] forwards every log record to an existing `log::Log`
//! implementation (typically the `simplelog` terminal/file logger) and also
//! pushes a formatted line into a [`LogRing`]. The UI polls `/api/logs?since=`
//! to read new lines since a monotonic cursor.

use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::stats::unix_now_ms;

/// Bounded ring buffer of recent log lines, keyed by a monotonic cursor.
///
/// On overflow, the oldest line is dropped so a log call never blocks.
pub struct LogRing {
    buffer: Mutex<VecDeque<Entry>>,
    cap: usize,
    next_cursor: AtomicU64,
}

#[derive(Clone)]
pub struct Entry {
    pub cursor: u64,
    pub ts_unix_ms: u64,
    pub level: &'static str,
    pub line: String,
}

impl LogRing {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            buffer: Mutex::new(VecDeque::new()),
            cap: cap.max(64),
            next_cursor: AtomicU64::new(1),
        }
    }

    pub fn push(&self, level: &'static str, line: String) {
        let cursor = self.next_cursor.fetch_add(1, Ordering::Relaxed);
        let entry = Entry {
            cursor,
            ts_unix_ms: unix_now_ms(),
            level,
            line,
        };
        if let Ok(mut guard) = self.buffer.lock() {
            while guard.len() >= self.cap {
                guard.pop_front();
            }
            guard.push_back(entry);
        }
    }

    /// Returns `(new_cursor, entries)` for every entry whose cursor is strictly
    /// greater than `since`. `new_cursor` is the next value the caller should
    /// pass back.
    #[must_use]
    pub fn read_since(&self, since: u64) -> (u64, Vec<Entry>) {
        let next = self.next_cursor.load(Ordering::Relaxed);
        let entries = self.buffer.lock().map_or_else(
            |_| Vec::new(),
            |g| g.iter().filter(|e| e.cursor > since).cloned().collect(),
        );
        (next, entries)
    }
}

/// `log::Log` adapter that tees every record through to `inner` while also
/// appending to a [`LogRing`].
pub struct TeeLogger {
    inner: Box<dyn log::Log>,
    level: log::LevelFilter,
    ring: std::sync::Arc<LogRing>,
}

impl TeeLogger {
    #[must_use]
    pub fn new(
        inner: Box<dyn log::Log>,
        level: log::LevelFilter,
        ring: std::sync::Arc<LogRing>,
    ) -> Self {
        Self { inner, level, ring }
    }
}

impl log::Log for TeeLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        self.ring
            .push(record.level().as_str(), format!("{}", record.args()));
        self.inner.log(record);
    }

    fn flush(&self) {
        self.inner.flush();
    }
}
