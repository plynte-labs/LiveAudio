# WebSocket Runtime Gaps (Rust migration regressions)

## Objective

Close behavior gaps in the Rust WebSocket path that were found while documenting the protocol (`docs/WEBSOCKET_OBS.md`, commit 000f7eb).

## Problem

1. `obs_enabled` is parsed in `crates/liveaudio-core/src/config/mod.rs` but never read. The Rust server always starts, so "transcript-only" mode (WS server off) that the Python engine supported is lost.
2. `is_replay` is `true` both for late-join replays (`server.rs:650`) and for live subtitles delayed more than 1 s (`replay.rs:67`). Clients that skip `is_replay` may drop delayed live subtitles.
3. The Rust server sends no keepalive pings (the Python server pings every 10 s). Half-open clients may not be detected.

## Authorized Scope and Constraints

- Scope: `crates/liveaudio-network`, `crates/liveaudio-core` (config/pipeline wiring), `crates/liveaudio-cli`, `desktop/src-tauri/src` only where needed to honor `obs_enabled`, the overlay `liveaudio/assets/subtitulos_obs.html` only if T2 requires it, plus `docs/WEBSOCKET_OBS.md` to keep the protocol doc accurate.
- T2 and T3 are investigate-first: implement only when a concrete defect is proven by a failing test. If not proven, record the finding and close the task without code.
- No new UI toggle for OBS (product decision, out of scope).
- Protocol changes must be additive (doc section 3 compatibility policy); `proto` stays 1.
- Never commit `desktop/src-tauri/gen/schemas/*`. No AI attribution in commits. No push.

## Execution Configuration

| Item | Value |
|---|---|
| Workflow | Organic Driven Development (ODD) |
| Branch | `fix/ws-runtime-gaps` (from `docs/websocket-protocol`) |
| TDD Mode | Strict TDD (RED -> GREEN -> REFACTOR), source: global CLAUDE.md |
| Test runner | `cargo test -p <crate>` (workspace: `cargo test --workspace`) |
| RDD | off (global) |
| Delivery strategy | ask-on-risk; forecast < 400 authored lines |

## Tasks

- [x] T1 — Honor `obs_enabled=false`: do not start the WebSocket server (desktop + CLI); pipeline and transcript keep working. Route: delegated (writer, Fable; 4+ files).
- [x] T2 — (not a defect) `is_replay` ambiguity: prove whether delayed live subtitles are dropped by the shipped overlay or documented clients; fix additively only if proven. Route: delegated (same writer).
- [x] T3 — (no ping/timeout added; finding recorded) Keepalive: prove whether dead/half-open clients leak or block the Rust server; add pings only if proven. Route: delegated (same writer).
- [x] T4 — Update `docs/WEBSOCKET_OBS.md` to match the final behavior. Route: delegated (same writer).

## Acceptance Criteria

- With `obs_enabled=false`, no WebSocket listener is bound; with `true`, behavior is unchanged.
- Each task has a RED test before its fix (or a recorded "not a defect" finding for T2/T3).
- `cargo test --workspace` passes; `cargo clippy --workspace --all-targets` has no new warnings.

## Progress / Evidence

- T1 (fixed), route: delegated-writer (single writer, inline in this run). Commit `f4183ef` (fix(network)), follow-up `a54f5ce` (test style).
  - RED: `cargo test -p liveaudio-network --test obs_enabled_gate` -> `E0432 unresolved import liveaudio_network::server::start_if_enabled`. Desktop: `cargo test -p liveaudio-desktop obs_` -> `obs_disabled_startup_does_not_bind_the_websocket` failed with `Failed to bind OBS WebSocket: All candidate ports in range 65535..65535 are unavailable`; `obs_disabled_status_reports_no_websocket_port` failed with `left: 8765, right: 0`. Frontend: `node test_frontend.js` 2 FAIL (WS pill off label, ws_off keys).
  - GREEN: gate 2/2, desktop obs_ 7/7, frontend 345/0 failed.
  - Scope: `start_if_enabled` (liveaudio-network) used by CLI `run_service` and desktop `ensure_startup_websocket`. Disabled: unbound server still accepts broadcasts; ws_port event and service-lock port not written; desktop status ws_port 0; overlay URL returns an error; WS pill shows `off` (i18n `ws_off`).
- T2 (not a defect). Evidence: the overlay (`liveaudio/assets/subtitulos_obs.html`) uses `is_replay` only to pick the stacked render mode (`resolveRenderStacked`, `enqueueSubtitle` ~L419-520). Its only discard is the MAX_PENDING_QUEUE transport valve, which does not depend on the flag. The documented example clients (section 8) skip flagged messages on purpose, and section 5.4 says so. Doc section 5.4 now states the overlay behavior. No wire change.
- T3 (no ping/timeout added; finding recorded). Scratch probe (deleted, not committed) in liveaudio-network: a connected client that never reads does not block `broadcast_raw` (600 frames in 8.8 ms, multi-thread runtime). A paced healthy client received 300/300 frames while a stalled client was present. Client memory is bounded (64 channel + 10 retry). Half-open clients stay counted until TCP gives up; no server timeout. Pings would not help: the blocked write stalls the same sink.
  - Decision gap (not fixed, outside T3's authorized fix type): with a zero-pace burst of 600 frames, the healthy client got 112 frames and never got the last one. Retry-queue frames only flush on the next `broadcast_raw`, so the last frames wait until another subtitle arrives. The doc now discloses this. Fix option: flush the retry queue from the per-client write loop after each send. This changes backpressure semantics, so it needs owner approval.
- T4 (done): `docs/WEBSOCKET_OBS.md` updated: section 4.5 (ws_port event), 5.4 (overlay uses is_replay for render only), 5.5 (retry tail, half-open clients), 6 (no pings, no idle timeout), 9 (obs_enabled honored by Rust desktop and CLI). Commit `2cf89c9`.
- Verification: `cargo test --workspace` 279 passed, 0 failed (after T1). Frontend `node test_frontend.js` 345 passed, 0 failed. Clippy: `cargo clippy --workspace --all-targets` fails on a pre-existing `not_unsafe_ptr_arg_deref` error in `liveaudio-core/src/job_object/mod.rs` (untouched). With that lint allowed, no lint lands in changed lines; the workspace has many pre-existing warnings.
- Engram mirror: pending (not written in this run). Feature document is the local source of truth.

## Next Step

Owner decision on the retry-tail flush (T3 finding), then review and push by the owner. Nothing pushed.
