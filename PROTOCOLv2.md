# Lidi Protocol v2 — Design Proposal

Status: **proposal** (not implemented). Target: replace the wire protocol described in
`PROTOCOL.md` while keeping the overall architecture (unidirectional UDP link, RaptorQ FEC,
multiplexed TCP/TLS/Unix streams) and the existing binaries' user-facing behaviour.

Terminology note: "reliable" here means *deterministic, attributable and bounded failure
handling with integrity guarantees* — not retransmission. The link remains physically
unidirectional; data lost beyond the FEC budget is permanently lost. What v2 guarantees is
that such a loss is (a) detected, (b) attributed to exactly the affected transfer, (c)
reported with a reason, and (d) never silently corrupts, stalls or crashes anything else.

---

## 1. Problems addressed

Every design change below is motivated by a documented v1 failure mode (references are to
`PROTOCOL.md`):

| # | v1 weakness | Reference | v2 mechanism |
|---|-------------|-----------|--------------|
| W1 | No session identity; sender restarts inferred by fragile heuristics (`restart_candidates`, `known_low_blocks`, stale-duplicate gap check) | §7.5, §9 | 64-bit random `session_id` in every datagram header |
| W2 | 8-bit `block_id` wraps at 256 → aliasing, 127-block window, `fast_track` false positives | §7.3, §7.12, §9 | 64-bit monotone `block_seq`, never wraps in practice |
| W3 | Decode failure unattributable → **all** transfers aborted on any lost block | §7.2, §9 | cleartext `client_id` in the datagram header → per-client abort |
| W4 | No integrity check on decoded blocks; corrupted `data_length` panics the process (`panic=abort`) | §7.13, §9 | XXH3-64 hash over every block; strict bounds validation |
| W5 | Config mismatch (mtu/block/repair) undetectable, surfaces as decode storm or garbage | §7.16, §9 | FEC parameters announced in every Beacon; receiver validates |
| W6 | `client_id` is u16, wraps, can collide with a live transfer | §7.14 | u32 per-session `client_id`, monotone, never reused within a session |
| W7 | Lost Start → orphaned `pending_start` entries, unbounded memory; lost End → stall until `abort_timeout` | §7.6, §7.7, §8 | Beacon carries the sender's active/ended transfer table → deterministic reconciliation |
| W8 | Heartbeat costs a full FEC block (~229 KB on wire) and carries no information | §7.10 | single-datagram control messages (Beacon), repeated for loss resilience |
| W9 | Multi-port: per-port windows/resets/restart detection with global abort blast radius | §7.18 | one global `block_seq` space and one logical reassembly shared by all ports |
| W10 | Silent stream truncation possible (blocks lost between End and dispatch, queue-full removals) | §7.7, §7.11 | End carries `total_bytes` + stream hash; every abnormal close carries a reason code |

Non-goals:
- No feedback channel, no retransmission, no ACKs (physical diode constraint).
- No confidentiality/authenticity on the UDP link (unchanged; TLS remains available at the
  stream endpoints). A keyed-hash extension point is reserved, see §12.
- No change to the file-level protocol (`lidi-file-send`/`lidi-file-receive` framing).

---

## 2. Design principles

1. **Everything the receiver needs to *route* and *account for* a block is in cleartext in
   every datagram** (session, block sequence, client, type). Only the payload needs decoding.
   FEC failure therefore never destroys routing information.
2. **Two planes**: a *data plane* (fixed-size FEC-encoded blocks, one per `block_seq`) and a
   *control plane* (small single-datagram messages, no FEC, redundancy by repetition).
3. **All receiver decisions are deterministic** functions of (received headers, beacon
   contents, configured timeouts). No accumulate-and-guess heuristics.
4. **Every queue and map on the receive side has a specified bound and a specified drop
   policy**, and every drop is observable (log + metric + downstream reason code).
5. **Failure blast radius is one client**, except for the loss of the link itself.

---

## 3. Wire format

All integers are little-endian. `mtu` ≤ 9000 as in v1.

### 3.1 Datagram header (all datagrams, both planes)

```
Offset  Size  Field
─────────────────────────────────────────────────────────────
0       1 B   magic        = 0xD1
1       1 B   version      = 0x02
2       1 B   flags        bit0: 1 = control plane, 0 = data plane
                           bit1: 1 = last packet of the block (hint only)
                           bits 2–7: reserved, must be 0
3       1 B   block_type   (see §3.3; cleartext copy of the block's type)
4       8 B   session_id   random, nonzero, fixed for a sender process lifetime
12      8 B   block_seq    data plane: block sequence number (see §3.2)
                           control plane: control sequence number (own counter)
20      4 B   client_id    0 for session-scoped messages (Beacon)
24      2 B   header_crc   CRC-16/CCITT over bytes [0, 24)
─────────────────────────────────────────────────────────────
Total: 26 bytes
```

Receiver validation, in order, on every datagram: `magic`, `version`, `header_crc`. Any
failure → drop the datagram, increment `lidi_receive_packets_rejected`. This also cleanly
rejects v1 traffic and foreign UDP noise (v1 packets start with a RaptorQ payload_id and
will practically never match magic+CRC).

### 3.2 Data plane: FEC packets and blocks

After the header, a data-plane datagram carries the RaptorQ `payload_id` (4 B) and one
encoding symbol, exactly as v1:

```
max_packet_size = (mtu - 28 - 26 - 4) rounded down to a multiple of 8
                   └ IP+UDP ┘ └hdr┘ └FEC┘
```

Defaults (`mtu=1500`): `max_packet_size = 1440` (v1: 1464).

The RaptorQ `source_block_number` (8-bit, inside `payload_id`) is set to
`block_seq mod 256` — it is still needed by the codec, but the receiver keys everything on
the 64-bit `block_seq` from the header, so the 8-bit wrap is harmless.

**Block layout** (the buffer that is FEC-encoded, always `transfer_length` bytes):

```
Offset  Size  Field
─────────────────────────────────────────────────────────────
0       8 B   session_id    (must equal header)
8       8 B   block_seq     (must equal header; binds content to wire position)
16      4 B   client_id     (must equal header)
20      8 B   client_seq    per-client sequence number (Start = 0)
28      1 B   block_type    (must equal header)
29      3 B   reserved      (must be 0)
32      4 B   data_length
36      var   payload       (data_length bytes)
…       pad   zero padding
end−8   8 B   block_hash    XXH3-64 over bytes [0, transfer_length − 8)
─────────────────────────────────────────────────────────────
Block overhead: 44 bytes (v1: 11)
max_data_len = transfer_length − 44
```

Receiver validation after decode, in order:
1. `block_hash` — mismatch → the decode produced garbage (aliasing, codec edge case):
   treat exactly like a decode failure (§6.3). **Never parse further.**
2. `data_length ≤ transfer_length − 44` — enforced structurally; the accessor returns
   `Err`, it can never slice out of bounds (fixes the v1 panic, W4).
3. `session_id`/`block_seq`/`client_id`/`block_type` equal the header values (majority of
   received packet headers) — mismatch → drop with metric (indicates header corruption that
   slipped past CRC, or a hash collision; both ~impossible, checked for defence in depth).

### 3.3 Block / message types

| Value | Name    | Plane   | client_id | Payload |
|-------|---------|---------|-----------|---------|
| 0x00  | Beacon  | control | 0         | see §5.2 |
| 0x01  | Start   | data    | ≠ 0       | `endpoint_id u16`, `flags u16` |
| 0x02  | Data    | data    | ≠ 0       | data chunk |
| 0x03  | End     | data    | ≠ 0       | `total_bytes u64`, `hash_algo u8` (0 = none, 1 = XXH3-128), `stream_hash 16 B` |
| 0x04  | Abort   | data    | ≠ 0       | `reason u8` (§7), `last_client_seq u64` |

Changes vs v1:
- **Heartbeat is replaced by Beacon** (control plane, see §5).
- **End no longer carries data.** The sender flushes any buffered bytes as a final Data
  block, then sends End with the transfer trailer. Rationale: End becomes small, fixed and
  cheap to mirror in beacons; a lost End no longer necessarily loses data (W10).
- `client_id = 0` is reserved for session-scoped messages; the per-client counter starts
  at 1.

### 3.4 Control plane datagrams

`flags.bit0 = 1`. After the 26-byte header:

```
0       2 B   body_length
2       var   body            (body_length bytes, ≤ max_packet_size)
…       8 B   body_hash       XXH3-64 over header ‖ body_length ‖ body
```

Control messages are **not** FEC-protected. Redundancy is by repetition: each control
message is sent `control_repeat` times (default 3), spaced ~10 ms apart, on **every**
configured UDP port. `block_seq` in the header is a dedicated control counter (per
repetition group, so duplicates are trivially deduplicated). Loss of an entire repetition
group is tolerated by design: beacons are periodic and cumulative (§5.2).

---

## 4. Sequencing model

- `session_id`: drawn from a CSPRNG at sender startup. Never 0.
- `block_seq` (u64): one global counter per sender process, stamped when the block is
  **produced** (client worker / beacon worker), *before* the encode fan-out. With multiple
  ports, encode workers on different ports consume blocks that already carry their final
  `block_seq` — the numbering is port-independent (W9). Wrap is unreachable (at 10 Gb/s and
  220 KB blocks: ~10^8 years).
- `client_id` (u32): global monotone counter per sender process, starting at 1. Not reused
  within a session; collision requires 2^32 connections in one process lifetime (W6).
- `client_seq` (u64): per-client, Start = 0, increments per block. No wrap in practice.

---

## 5. Session layer

### 5.1 Session lifecycle

A *session* is one sender process lifetime. The receiver tracks exactly one
`current_session_id` per link:

- **Adoption** (cold start or after silence): the first datagram whose header validates
  defines the candidate session. To resist a forged/corrupted stray packet, adoption
  requires either one valid control message (body_hash checks out) or data-plane packets
  for the same `session_id` from ≥ 2 distinct `block_seq` values.
- **Switchover** (W1): datagrams with a different `session_id` while a session is current.
  Same confirmation rule as adoption. On confirmation:
  1. every in-flight transfer of the old session is aborted with reason
     `SessionReplaced`, partial reassembly state is discarded;
  2. the new session is adopted; reassembly starts at the lowest `block_seq` seen for it.
  Old-session packets still in flight after switchover are dropped by `session_id` filter
  (no heuristics, no `restart_candidates`, no stale-duplicate gap checks).
- **Loss of link**: no valid datagram for `reset_timeout` (kept, default 2 s) → all
  transfers aborted with reason `LinkSilent`; session stays adopted (a reappearing sender
  with the same `session_id` resumes; a new one triggers switchover).

### 5.2 Beacon

Sent by the sender every `beacon_interval` (default **1 s**; cheap — ~150 bytes × repeats,
vs ~229 KB for a v1 heartbeat block, W8). Body:

```
0       2 B   params: mtu
2       4 B   params: transfer_length
6       2 B   params: symbol_count
8       2 B   params: nb_repair_packets
10      1 B   params: nb_ports
11      1 B   params: endpoint_count       (number of `from` endpoints configured)
12      4 B   beacon_interval_ms
16      8 B   highest_block_seq            (last block_seq produced, 0 if none yet)
24      1 B   active_count (= A)
25      A ×   { client_id u32, endpoint_id u16, last_client_seq u64 }   (14 B each)
…       1 B   ended_count (= E, the last up-to-8 transfers that ended)
…       E ×   { client_id u32, final_client_seq u64, status u8 }        (13 B each)
                status: 0 = completed, 1 = aborted
```

The beacon gives the receiver, without decoding anything:

- **Parameter validation** (W5): on adoption and on every beacon, the receiver compares
  `mtu / transfer_length / symbol_count / nb_repair_packets / endpoint_count` with its own
  configuration. Mismatch → log at ERROR with both value sets, refuse the session (drop its
  data packets), raise `lidi_receive_param_mismatch`. This converts the v1 silent decode
  storm into one explicit, actionable message.
- **Liveness**: beacon arrival refreshes the link-silence timer even on an idle link, on
  every port (v1 heartbeats reached only one port, §7.18).
- **Tail-loss detection**: `highest_block_seq` advances the reassembly horizon even when
  all data packets of the tail blocks were lost (§6.3).
- **Reconciliation** (W7): the active/ended tables let the receiver recover from lost
  Start/End/Abort blocks (§6.4).

---

## 6. Receiver pipeline

The thread topology is unchanged (udp × port → reassembly → dispatch → clients), with these
redefinitions:

### 6.1 UDP workers

Parse and validate the 26-byte header (magic, version, CRC). Route control-plane datagrams
(after `body_hash` check and dedup) to dispatch directly; route data-plane packets to the
reassembly stage tagged with their validated header. Reject everything else (metric).

### 6.2 Reassembly (replaces reblock)

One **logical** reassembly instance per session, shared by all ports (implementation may
shard by `block_seq mod N` since blocks are independent; W9). State:

```
pending: BTreeMap<u64 /* block_seq */, Pending {
    client_id: u32, block_type: u8,          // from packet headers
    packets: Vec<EncodingPacket>,
    first_seen: Instant,
}>
horizon: u64      // max(highest block_seq seen in packet headers, beacon.highest_block_seq)
delivered_below: u64   // all block_seq < this are decoded or given up
```

Rules:
- A packet for `block_seq < delivered_below` or for an already-completed block: drop
  (late duplicate; exact arithmetic on u64, no aliasing possible — W2).
- When `pending[s].packets.len() ≥ min_nb_packets`: decode, verify block hash (§3.2),
  deliver `Ok(block)` to dispatch. Delivery order across blocks is unordered by design
  (as in v1 multi-port); per-client ordering is downstream's job via `client_seq`.
- **Give-up** — block `s` is declared lost when any of:
  - `horizon − s > window_blocks` (default 1024 blocks ≈ 225 MB in flight; configurable), or
  - `now − first_seen > block_timeout` (default 500 ms), or
  - `pending.len() > window_blocks` (hard memory bound; give up the **oldest** first).

  On give-up, emit `Lost { block_seq: s, client_id, block_type }` to dispatch, using the
  cleartext header info (W3), then drop the packets. There is **no** global `None`, no
  `fast_track`, no forced decode of partial blocks (W2): a v1 "probable network interrupt"
  simply becomes per-block timeouts whose `Lost` events name their victims.

Memory bound: `window_blocks × nb_packets × mtu` ≈ 240 MB worst case with defaults;
`window_blocks` should be sized to the bandwidth-delay of the deployment (a LAN diode needs
far less; the default suits 10 Gb/s with ~200 ms of tolerated stall).

### 6.3 Dispatch

Consumes three inputs: decoded blocks, `Lost` events, beacons. Per-client state machine
(replaces `active_transfers` + `pending_start`, W7):

```
                    Start decoded
   (unknown) ────────────────────────────► Active
       │                                      │ End decoded (trailer verified)
       │ Data/End before Start                │──────────────► Closed(Completed)
       ▼                                      │ Abort decoded / Lost(client) /
   AwaitingStart(buffer, ttl)                 │ beacon reconciliation / local error
       │ Start decoded → replay buffer        │──────────────► Closed(Aborted(reason))
       │ beacon: client active with           │
       │   endpoint E → promote to Active     │
       │ ttl expired / buffer full            │
       └──────────────► dropped (metric, log) │
```

- **AwaitingStart** is the successor of `pending_start`, but bounded and mortal: at most
  `awaiting_start_max` blocks (default 256) and `awaiting_start_ttl` (default
  2 × beacon_interval). A lost Start is repaired by the next beacon's active table
  (endpoint_id is in there), so the buffer only needs to cover one beacon period.
- **`Lost { client_id, … }`**: abort exactly that client with reason `BlockLost`
  (blast radius = 1, W3). Other transfers are untouched.
- **Beacon reconciliation**, on every beacon:
  - client in beacon-active but locally unknown → lost Start; create the transfer from the
    beacon's `endpoint_id`, state Active, expecting `client_seq` continuity checks to
    surface any lost data (they will: the gap triggers `Lost` or reorder timeout).
  - client locally Active but in beacon-ended with `status=completed` → its End was lost.
    If the receiver has delivered every `client_seq < final_client_seq`, close the output
    cleanly (flush + FIN) and log `closed via beacon (End block lost, trailer
    unverified)`; else abort with `BlockLost`.
  - client locally Active but absent from both tables for 2 consecutive beacons → the
    sender finished it and we missed everything relevant → abort, reason `Reconciled`.
- **Queue-full** (v1 §7.7): unchanged mechanically (`try_send`), but the failure path is
  now explicit. A synthetic Abort cannot be enqueued into a channel that is already full,
  so dispatch records the reason (`ReceiverOverrun`) in the client's state, removes the
  entry and drops the channel sender; the reorder worker translates the resulting
  disconnect into a downstream abort carrying that recorded reason (§6.4, §6.5). No
  orphaned reorder worker, no `pending_start` leak, no silent RST.
- **End trailer** (W10): on End, after the last Data block is delivered, verify
  `total_bytes` against the delivered byte count (and `stream_hash` when enabled). Mismatch
  → abort with reason `TrailerMismatch` instead of silently closing a truncated stream.

### 6.4 Client reorder

As v1 (park by `client_seq`, forward in order), with:
- u64 sequence space (duplicate seq now indicates real corruption; still an error, but it
  aborts only this client with reason `ProtocolViolation` instead of killing the socket
  without `client_end`);
- Abort still bypasses ordering (v1 §7.9) and now carries `reason` + `last_client_seq`,
  both logged;
- channel disconnect from dispatch is translated into a downstream abort with the reason
  provided by dispatch (or `Internal` if none), and `client_end(conn, false)` is **always**
  called — no more silent socket drops (v1 §7.7/§7.8 left the socket to RST).

### 6.5 Abort reasons

```
0x01 SenderIo          sender-side read error (v1's only real Abort)
0x02 BlockLost         FEC gave up on a block of this transfer
0x03 SessionReplaced   sender restarted
0x04 LinkSilent        reset_timeout with no valid datagrams
0x05 ReceiverOverrun   receive-side queue full / downstream too slow
0x06 TrailerMismatch   End accounting or stream hash failed
0x07 Reconciled        beacon says the transfer no longer exists
0x08 ProtocolViolation malformed/duplicate sequencing
```

Reasons appear in logs and as labels on `lidi_receive_transfers_aborted{reason=…}`.

---

## 7. Failure-mode matrix (v1 → v2)

| v1 case (PROTOCOL.md) | v1 outcome | v2 outcome |
|---|---|---|
| §7.1 loss < FEC budget | invisible | invisible (unchanged) |
| §7.2 loss > FEC budget | **all** transfers aborted | one `Lost` event → only the owning client aborted, reason `BlockLost` |
| §7.3 fast_track false positive | spurious global abort under load | mechanism removed; give-up is exact (u64 horizon + timers) |
| §7.4 sender killed, silence | global abort after 2 s, window re-anchor heuristic | per-client aborts reason `LinkSilent`; session survives for same-id resume; new sender = clean switchover |
| §7.5 sender restart mid-transfer | `restart_candidates` heuristic, catch-up races, flaky | `session_id` switchover on first confirmed datagrams; deterministic |
| §7.6 pending_start race / orphan leak | unbounded, cleared only on sync loss | `AwaitingStart` bounded + TTL + beacon repair of lost Start |
| §7.7 queue full at client | silent removal, reorder stalls, socket RST | explicit abort reason `ReceiverOverrun`, orderly `client_end` |
| §7.8 abort_timeout | last-resort timeout, no cause info | retained as last resort; most cases resolved earlier by reasons above |
| §7.9 Abort(seq=0) bypass hack | needed because dispatch has no seq | Abort carries `last_client_seq` + reason; bypass retained but informative |
| §7.10 heartbeat = full block, log-only | ~229 KB/beat, one port only | Beacon: ~150 B, all ports, parameter/reconciliation payload |
| §7.11 dispatch blocking on to_clients | possible silent drop of new transfers | unchanged concurrency, but bounded queues are mandatory in v2 and overflow maps to `ReceiverOverrun` |
| §7.12 receiver lag > window | global sync loss | oldest blocks given up individually (`BlockLost` per client); horizon exact |
| §7.13 corrupted decode | garbage parsed; worst case **process abort** | block hash → treated as decode failure; bounds-checked accessors; no panic path |
| §7.14 client_id wrap collision | cross-wired streams, cleanup race | u32 per-session ids, no reuse |
| §7.15 invalid endpoint | orphaned entries, silent data loss | endpoint_count validated per beacon at session adoption; per-client abort otherwise |
| §7.16 config mismatch | permanent decode storm, undiagnosable | session refused with explicit ERROR naming both parameter sets |
| §7.17 kernel UDP drops | indistinguishable from network loss | still possible (physics), but blast radius per-client; sysctl guidance unchanged |
| §7.18 multi-port interference | per-port windows, global aborts, broken restart detection | single seq space, shared reassembly, beacons on all ports |

---

## 8. Overhead analysis (defaults: mtu 1500, block 220 000, repair 1 %)

|  | v1 | v2 |
|---|---|---|
| per-packet header (after IP/UDP) | 4 B (FEC) | 30 B (26 hdr + 4 FEC) |
| `max_packet_size` | 1464 | 1440 |
| symbols / block | 150 | 152 |
| packets / block (with repair) | 153 | 156 |
| per-block overhead | 11 B | 44 B |
| usable payload / block | 219 589 B | 218 836 B |
| wire bytes / block (incl. IP/UDP) | 228 888 B | 233 688 B |
| **total overhead vs payload** | **4.2 %** | **6.8 %** |
| idle-link cost per beat | ~229 KB (heartbeat block) | ~450 B (beacon × 3 repeats) per port |

The +2.6 points of data-plane overhead buy per-client attribution, integrity and session
identity; jumbo frames (`mtu 9000`) reduce v2 total overhead to ~1.6 %. Idle-link cost
drops by three orders of magnitude.

---

## 9. Configuration

New/changed parameters (both sides unless noted):

| Parameter | Default | Description |
|-----------|---------|-------------|
| `beacon_interval` | 1 s | Beacon period (send). Replaces `heartbeat`. 0 = disabled (not recommended; disables W5/W7 repairs) |
| `control_repeat` | 3 | Repetitions of each control message |
| `window_blocks` | 1024 | Reassembly horizon and hard memory bound (receive) |
| `block_timeout` | 500 ms | Age at which an incomplete block is given up (receive) |
| `awaiting_start_ttl` | 2 × beacon_interval | Lifetime of a transfer awaiting its Start (receive) |
| `awaiting_start_max` | 256 blocks | Buffer bound per awaiting transfer (receive) |
| `reset_timeout` | 2 s | Unchanged meaning: link-silence threshold (receive) |
| `abort_timeout` | 20 s recommended | Unchanged: last-resort per-client stall guard (receive) |
| `queue_size` | **4096 (now bounded by default)** | Per-client queue; 0/unbounded no longer allowed in v2 (W7, §6.3) |

Removed: `heartbeat` (both sides), all reliance on matching heartbeat intervals.

---

## 10. Metrics (additions)

```
lidi_receive_packets_rejected{cause="magic"|"crc"|"session"}
lidi_receive_blocks_given_up{cause="horizon"|"timeout"|"memory"}
lidi_receive_transfers_aborted{reason=<§6.5 reason>}
lidi_receive_param_mismatch
lidi_receive_beacon_age_seconds
lidi_receive_transfers_repaired{kind="start_from_beacon"|"end_from_beacon"}
```

---

## 11. Compatibility and migration

- v1 and v2 cannot interoperate. The magic/version bytes make v2 receivers reject v1
  traffic deterministically (and v1 receivers will fail to decode v2 packets, producing its
  usual decode-failure storm — do not mix them on one port).
- Migration: deploy the v2 receiver on new UDP ports alongside the v1 receiver, switch the
  sender, then retire the v1 ports. The diode being one-way, sender and receiver must be
  upgraded in a coordinated window (receiver first).
- `version` byte gives room for v3+ negotiation-free evolution (a receiver may support
  several versions simultaneously since every datagram is self-describing).

---

## 12. Reserved extensions (out of scope, but the format allows them)

- **Authentication**: `flags.bit2` + replacing XXH3-64 hashes with a keyed BLAKE3/HMAC
  truncated to 8 B (pre-shared key per link) would authenticate both planes without format
  changes.
- **Packet interleaving**: transmitting the packets of D consecutive blocks round-robin
  (`interleave = D`) to spread burst losses across blocks; receiver needs no change
  (reassembly is already unordered). Latency cost: D − 1 blocks.
- **Per-endpoint FEC profiles**: `repair` per `from` endpoint, announced in the beacon
  params table.

---

## 13. Implementation plan

### Phase 1 — `lidi-protocol`: v2 formats (no behaviour change elsewhere)
- New module `v2`: `PacketHeader` (ser/de + CRC-16), `Block` (new layout, XXH3-64, strictly
  bounds-checked accessors returning `Result`), `ControlMessage`/`Beacon` (ser/de +
  body_hash), reason codes. Add `xxhash-rust` (already used by `lidi-command-utils`) and a
  small CRC-16 implementation (no new heavy deps).
- Unit tests: round-trips, truncation/corruption fuzz (every field), hash/CRC rejection,
  `data_length` out-of-range must return `Err` (regression test for the v1 panic).

### Phase 2 — `lidi-send`
- `Sender`: add `session_id` (rand), `AtomicU64` block_seq stamped at block creation
  (move the counter out of `encode.rs`), u32 client_id counter.
- `client.rs`: new block layout; End split (flush Data, then trailer-only End); stream
  byte/hash accounting for the trailer.
- `encode.rs`: prepend `PacketHeader` to each UDP payload (header bytes are per-block
  constant except nothing — fully precomputable once per block).
- Replace `heartbeat.rs` with `beacon.rs`: maintains active/ended tables (fed by
  server/client workers over a small channel), serializes, hands to **every** port's UDP
  worker with repetition.

### Phase 3 — `lidi-receive`
- `udp.rs`: header validation + plane routing.
- Replace `reblock.rs` with `reassembly.rs` (§6.2): `BTreeMap`, horizon, give-up rules,
  `Lost` events. Delete `restart_candidates`, `known_low_blocks`, `fast_track`,
  `flush_window`.
- `dispatch.rs`: per-client state machine (§6.3), beacon handling, reconciliation,
  reason-carrying aborts. Delete `pending_start`.
- `client_reorder.rs` / `client.rs`: u64 seq, reason propagation, guaranteed `client_end`.

### Phase 4 — integration tests (`features/`)
- Port existing scenarios (nominal, loss, throughput, multi-client, stability) to v2.
- New scenarios from the matrix in §7: single-client abort on loss (other client
  unaffected), sender restart under traffic (deterministic switchover), lost Start repaired
  by beacon, lost End closed via beacon, parameter mismatch refusal, receiver overrun
  reason, multi-port with loss on one port only.

### Order-of-magnitude estimate
Phases 1–3 are each a few days of work; the reassembly + dispatch rewrite (~600 lines
replaced) is the core risk and is exactly the code whose v1 flakiness motivates v2. The v1
integration suite provides the safety net; the protocol crate can land first with both v1
and v2 modules coexisting.
