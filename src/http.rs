//! Minimal, dependency-free HTTP/1.1 server that exposes a read-only
//! observability UI for a single `diode-send` or `diode-receive` instance.
//!
//! This intentionally does not pull in a full HTTP crate or an async runtime.
//! The schemas are tiny, fully controlled by us, and serialised by a hand-rolled
//! JSON writer. Concurrency is single-threaded: the UI polls at 1 Hz and
//! handlers are sub-millisecond.
//!
//! Routes served when bound:
//! - `GET /`            → embedded `index.html`
//! - `GET /api/info`    → static config snapshot
//! - `GET /api/status`  → live counters + active clients
//! - `GET /api/logs`    → recent log lines (optional `?since=<u64>`)

use std::{
    fmt::Write as FmtWrite,
    io::{BufRead, BufReader, Read, Write},
    net::{self, SocketAddr, TcpListener, TcpStream},
    path,
    sync::Arc,
    time::Duration,
};

use crate::{
    logring::LogRing,
    stats::{ClientEntry, Stats, StatsSnapshot},
};

const INDEX_HTML: &str = include_str!("web/index.html");

const READ_TIMEOUT: Duration = Duration::from_secs(2);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUEST_BYTES: usize = 8 * 1024;

/// Static configuration captured once at startup and echoed to the UI.
pub struct InfoSnapshot {
    pub role: &'static str,
    pub version: &'static str,
    pub max_clients: u32,
    pub block_bytes: u32,
    pub repair_pct: u32,
    pub heartbeat_secs: Option<u64>,
    pub mtu: u16,
    pub peer: Option<SocketAddr>,
    pub bind: Option<SocketAddr>,
    pub listener_tcp: Option<SocketAddr>,
    pub listener_unix: Option<path::PathBuf>,
    pub forward_tcp: Option<SocketAddr>,
    pub forward_unix: Option<path::PathBuf>,
    pub flush: bool,
    pub hash: bool,
}

/// Bind and run the HTTP server on `addr`. If `log_ring` is `Some`, the
/// `/api/logs` endpoint serves entries from it; otherwise it returns an empty
/// list.
///
/// Returns once the listener stops accepting (which currently only happens on
/// fatal I/O error). The caller typically spawns this on a dedicated thread.
pub fn start(
    addr: SocketAddr,
    stats: &Arc<Stats>,
    info: &Arc<InfoSnapshot>,
    log_ring: Option<&Arc<LogRing>>,
) {
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            log::error!("observability HTTP failed to bind {addr}: {e}");
            return;
        }
    };
    log::info!("observability HTTP listening on http://{addr}/");

    let ring_ref = log_ring.map(AsRef::as_ref);
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => handle(stream, stats, info, ring_ref),
            Err(e) => {
                log::warn!("observability HTTP accept error: {e}");
            }
        }
    }
}

fn handle(
    mut stream: TcpStream,
    stats: &Stats,
    info: &InfoSnapshot,
    log_ring: Option<&LogRing>,
) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);

    let request = match read_request(&stream) {
        Ok(r) => r,
        Err(e) => {
            log::debug!("observability HTTP read error: {e}");
            return;
        }
    };

    let (method, target) = parse_request_line(&request);
    if method != "GET" {
        write_response(
            &mut stream,
            405,
            "Method Not Allowed",
            "application/json; charset=utf-8",
            br#"{"error":"method not allowed"}"#,
        );
        return;
    }

    let (path, query) = split_query(target);

    match path {
        "/" => write_response(
            &mut stream,
            200,
            "OK",
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
        ),
        "/api/info" => {
            let body = render_info(info);
            write_response(
                &mut stream,
                200,
                "OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
            );
        }
        "/api/status" => {
            let snapshot = stats.snapshot();
            let body = render_status(stats.role, &snapshot);
            write_response(
                &mut stream,
                200,
                "OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
            );
        }
        "/api/logs" => {
            let since = parse_since(query);
            let body = render_logs(log_ring, since);
            write_response(
                &mut stream,
                200,
                "OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
            );
        }
        _ => write_response(
            &mut stream,
            404,
            "Not Found",
            "application/json; charset=utf-8",
            br#"{"error":"not found"}"#,
        ),
    }
}

fn read_request(stream: &TcpStream) -> std::io::Result<String> {
    let mut reader = BufReader::new(stream).take(MAX_REQUEST_BYTES as u64);
    let mut head = String::new();
    // Read lines until the blank CRLF that ends the header block. The browser
    // still sends headers even though we ignore them; we just need to drain
    // them so the socket is ready for the response.
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        if line == "\r\n" || line == "\n" {
            if head.is_empty() {
                // Client sent only CRLF: treat as an empty line preceding the
                // request. Loop again.
                continue;
            }
            break;
        }
        if head.is_empty() {
            head.push_str(&line);
        }
    }
    Ok(head)
}

fn parse_request_line(line: &str) -> (&str, &str) {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.splitn(3, ' ');
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    (method, target)
}

fn split_query(target: &str) -> (&str, &str) {
    target.find('?').map_or((target, ""), |i| {
        let (p, q) = target.split_at(i);
        let q = q.get(1..).unwrap_or("");
        (p, q)
    })
}

fn parse_since(query: &str) -> u64 {
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("since=")
            && let Ok(n) = value.parse::<u64>()
        {
            return n;
        }
    }
    0
}

fn write_response(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) {
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\
         \r\n",
        len = body.len(),
    );
    if let Err(e) = stream.write_all(header.as_bytes()) {
        log::debug!("observability HTTP write header error: {e}");
        return;
    }
    if let Err(e) = stream.write_all(body) {
        log::debug!("observability HTTP write body error: {e}");
        return;
    }
    let _ = stream.flush();
    let _ = stream.shutdown(net::Shutdown::Both);
}

// --- JSON rendering ---------------------------------------------------------

fn render_info(info: &InfoSnapshot) -> String {
    let mut out = String::with_capacity(512);
    out.push('{');
    field_str(&mut out, "role", info.role, true);
    field_str(&mut out, "version", info.version, false);
    field_u64(&mut out, "max_clients", u64::from(info.max_clients), false);
    field_u64(&mut out, "block_bytes", u64::from(info.block_bytes), false);
    field_u64(&mut out, "repair_pct", u64::from(info.repair_pct), false);
    out.push(',');
    push_key(&mut out, "heartbeat_secs");
    match info.heartbeat_secs {
        Some(s) => {
            let _ = write!(out, "{s}");
        }
        None => out.push_str("null"),
    }
    field_u64(&mut out, "mtu", u64::from(info.mtu), false);
    field_opt_string(&mut out, "peer", info.peer.map(|a| a.to_string()), false);
    field_opt_string(&mut out, "bind", info.bind.map(|a| a.to_string()), false);
    field_bool(&mut out, "flush", info.flush, false);
    field_bool(&mut out, "hash", info.hash, false);
    out.push(',');
    push_key(&mut out, "listeners");
    out.push('{');
    field_opt_string(
        &mut out,
        "tcp",
        info.listener_tcp.map(|a| a.to_string()),
        true,
    );
    field_opt_string(
        &mut out,
        "unix",
        info.listener_unix
            .as_ref()
            .map(|p| p.display().to_string()),
        false,
    );
    out.push('}');
    out.push(',');
    push_key(&mut out, "forward");
    out.push('{');
    field_opt_string(
        &mut out,
        "tcp",
        info.forward_tcp.map(|a| a.to_string()),
        true,
    );
    field_opt_string(
        &mut out,
        "unix",
        info.forward_unix.as_ref().map(|p| p.display().to_string()),
        false,
    );
    out.push('}');
    out.push('}');
    out
}

fn render_status(role: &str, snap: &StatsSnapshot) -> String {
    let mut out = String::with_capacity(512 + snap.clients.len() * 96);
    out.push('{');
    field_str(&mut out, "role", role, true);
    field_u64(&mut out, "uptime_secs", snap.uptime_secs, false);
    field_u64(&mut out, "now_unix_ms", snap.now_unix_ms, false);
    field_u64(&mut out, "bytes_total", snap.bytes_total, false);
    field_u64(&mut out, "packets_total", snap.packets_total, false);
    field_u64(&mut out, "transfers_started", snap.transfers_started, false);
    field_u64(&mut out, "transfers_finished", snap.transfers_finished, false);
    field_u64(&mut out, "transfers_aborted", snap.transfers_aborted, false);
    // `active_count` is a `usize`; cast narrowly with `u64::try_from` so we
    // don't silently wrap on 128-bit-of-the-future targets.
    field_u64(
        &mut out,
        "active_count",
        u64::try_from(snap.active_count).unwrap_or(u64::MAX),
        false,
    );
    field_u64(
        &mut out,
        "last_heartbeat_unix_ms",
        snap.last_heartbeat_unix_ms,
        false,
    );
    out.push(',');
    push_key(&mut out, "clients");
    out.push('[');
    for (i, c) in snap.clients.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        render_client(&mut out, c);
    }
    out.push(']');
    out.push('}');
    out
}

fn render_client(out: &mut String, c: &ClientEntry) {
    out.push('{');
    push_key(out, "id");
    out.push('"');
    let _ = write!(out, "{:x}", c.id);
    out.push('"');
    field_u64(out, "started_unix_ms", c.started_unix_ms, false);
    field_u64(out, "bytes", c.bytes, false);
    field_str(out, "state", c.state.as_str(), false);
    out.push('}');
}

fn render_logs(ring: Option<&LogRing>, since: u64) -> String {
    let Some(ring) = ring else {
        return String::from(r#"{"cursor":0,"lines":[]}"#);
    };
    let (cursor, entries) = ring.read_since(since);
    let mut out = String::with_capacity(128 + entries.len() * 96);
    out.push('{');
    push_key(&mut out, "cursor");
    let _ = write!(out, "{cursor}");
    out.push(',');
    push_key(&mut out, "lines");
    out.push('[');
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('{');
        push_key(&mut out, "cursor");
        let _ = write!(out, "{}", e.cursor);
        field_u64(&mut out, "ts_unix_ms", e.ts_unix_ms, false);
        field_str(&mut out, "level", e.level, false);
        field_str(&mut out, "msg", &e.line, false);
        out.push('}');
    }
    out.push(']');
    out.push('}');
    out
}

// --- tiny JSON helpers ------------------------------------------------------

fn push_key(out: &mut String, key: &str) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
}

fn field_str(out: &mut String, key: &str, value: &str, first: bool) {
    if !first {
        out.push(',');
    }
    push_key(out, key);
    write_escaped_string(out, value);
}

fn field_u64(out: &mut String, key: &str, value: u64, first: bool) {
    if !first {
        out.push(',');
    }
    push_key(out, key);
    let _ = write!(out, "{value}");
}

fn field_bool(out: &mut String, key: &str, value: bool, first: bool) {
    if !first {
        out.push(',');
    }
    push_key(out, key);
    out.push_str(if value { "true" } else { "false" });
}

fn field_opt_string(out: &mut String, key: &str, value: Option<String>, first: bool) {
    if !first {
        out.push(',');
    }
    push_key(out, key);
    match value {
        Some(s) => write_escaped_string(out, &s),
        None => out.push_str("null"),
    }
}

fn write_escaped_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
