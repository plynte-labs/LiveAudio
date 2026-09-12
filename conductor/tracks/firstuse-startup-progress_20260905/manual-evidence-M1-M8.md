# Manual Evidence Sheet M1–M8 — firstuse-startup-progress_20260905

> [Agent QA Qwen 3.6Plus] — manual validation sheet. If a ticket cannot be manually validated, it is not closable.
> Traceability: `spec.md` (T1–T4 acceptance blocks), `tasks.md:40-66` (M1–M8 matrix), Decisions D1–D8 + REQ-6 option (a) pre-import heartbeat.

## Cover

| Field | Value |
|---|---|
| Project | `liveaudio` |
| Track | `firstuse-startup-progress_20260905` |
| Branch | `feature/firstuse-startup-progress` |
| Checkpoint | `41bb1bb` |
| Date | 2026-09-06 (UTC) |
| Tester | _<name / initials>_ |
| Build | _<commit `41bb1bb` + how it was launched: `python main.py` / service mode / exe>_ |
| Model under test | `small` (unless noted) |
| Result summary | _M1 [ ] / M2 [ ] / M3 [ ] / M4 [ ] / M5 [ ] / M6 [ ] / M7 [ ] / M8 [ ]_ |

## Prerequisites

1. Clean build/install from branch `feature/firstuse-startup-progress @ 41bb1bb`. Confirm with `git status --short --branch` (read-only; do NOT commit, push, merge, or revert anything).
2. OBS Studio with a Browser Source pointing at `subtitulos_obs.html`. Keep OBS open and visible during every ticket.
3. Default config except where a ticket says otherwise: model `small`, prewarm default `true`, backlog policy `auto` (with `send_all` capped at 256), stall window default 120–180s (only M3 overrides it temporarily).
4. Network ON for M1–M4, M6–M8; network OFF only for M5.
5. For M1 (fresh download): either rename the model cache folder or select a model variant not yet downloaded, so provisioning actually downloads.
6. Recording kit per ticket: screenshot of the ASR status pill, OBS window state, and the relevant log lines / health snapshot / counters. No audio, transcripts, secrets, API keys, or PII in evidence.
7. After M3: revert the temporary `startup_stall_sec: 5` override back to the default before M7/M8.

---

## M1 — Visible download with monotonic %

**Objective:** First-use Whisper download shows honest `downloading` progress 0–100, never regressing within one attempt.

**Given:** Model cache does NOT contain the `small` model (cache folder renamed or undownloaded model selected).
**When:** The app starts provisioning and the supervisor pumps engine progress events `{phase,percent,attempt,code}` to the GUI.
**Then:** The ASR pill reads `ASR: descargando N%` / `ASR: downloading N%` with N in 0–100 and monotonically non-decreasing.
**Error Path:** Unparseable tqdm lines fall back to indeterminate `downloading…` text — never a crash, never a frozen `0%`.
**UI State:** Status pill = Downloading N% (localized key); no progress bar (pill only, per D6).
**OBS Behavior:** No subtitles yet, no errors in the overlay; overlay stays silent until `ready`.

**Steps:**
1. Prepare an empty model cache (rename cache dir or pick an undownloaded model).
2. Launch the app from this branch and open the Model/ASR status area.
3. Observe the pill continuously from start until `ready`.
4. Record at least 5 samples of N (e.g. 0 → 12 → 38 → 71 → 100) with timestamps.
5. Confirm N never goes backward (compare consecutive samples).

**Evidence to record:**
- Screenshot(s) of pill at low/mid/high %.
- Log lines with `phase=downloading`, `percent`, `attempt` (same attempt throughout).
- Note any indeterminate fallback text if it appears.

- [ ] PASS / [ ] FAIL — Notes: _______________

## M2 — Slow progress: warning only, never kill

**Objective:** A slow-but-progressing download is never killed; the ASR worker stays alive and % keeps advancing.

**Given:** A download in progress (state from M1, same attempt).
**When:** Progress arrives slowly (long gaps but nonzero advancement before the 120–180s stall window expires).
**Then:** The supervisor keeps the worker alive, % keeps advancing, and at most an advisory "taking longer than expected — still working" copy appears; no restart, no `stalled`, no process kill.
**Error Path:** Even under very slow throughput, expiry of any advisory deadline alone never kills a progressing download; `stalled` requires zero events/progress for the full stall window.
**UI State:** Pill still shows Downloading N% (optionally plus localized slow-warning copy); Retry button stays hidden.
**OBS Behavior:** Silence until `ready`; no bursts, no partial subtitles.

**Steps:**
1. Start from M1 conditions (download in progress).
2. Let it run without touching Retry or restarting (throttle network if needed to slow it, do not block it fully).
3. Verify the ASR worker process stays alive for the whole download.
4. Verify % advances (even slowly) and never resets.
5. Confirm no `stalled` state and no Retry button appears.

**Evidence to record:**
- Start/end timestamps + % samples showing slow advance.
- Worker-alive proof (process list or supervisor log showing no restart for that attempt).
- Screenshot of pill mid-download with no Retry button.

- [ ] PASS / [ ] FAIL — Notes: _______________

## M3 — Stall → stalled + Retry (temporary 5s window)

**Objective:** A hung provisioning run surfaces `stalled` with a manual Retry offer, using the TEMPORARY test-only override.

**Given:** A hung/stuck download or blocked cache with the live process still running.
**When:** `startup_stall_sec: 5` elapses with zero events/progress (TEST-ONLY override; default is 120–180s).
**Then:** Supervisor surfaces `stalled` with code `provision-timeout-stalled` and a visible manual Retry button; pill reads `ASR: detenido. Pulsa Reintentar.` / `ASR: stalled. Press Retry.`
**Error Path:** The advisory absolute deadline alone does not kill anything; only the zero-progress stall condition enters `stalled`, and entry never restarts the worker by itself.
**UI State:** Stalled pill + one-line remediation + Retry button visible (Retry visible ONLY in `stalled`/`failed`).
**OBS Behavior:** Silence; on later recovery the backlog policy governs catch-up (no burst — see M8).

**Steps:**
1. Set the temporary override `startup_stall_sec: 5` in config (note the exact key/value and file).
2. Create the hung condition (blocked download or locked cache) and start the app.
3. Wait > 5s with zero progress; observe the pill.
4. Confirm the `stalled` pill text + Retry button + `provision-timeout-stalled` in logs.
5. Leave the app in `stalled` for M4 (do NOT revert the override yet).

**Evidence to record:**
- Screenshot of stalled pill + Retry button.
- Log line with `stalled` + `provision-timeout-stalled` + stall elapsed ≥ 5s.
- Config snippet showing the temporary `startup_stall_sec: 5`.

- [ ] PASS / [ ] FAIL — Notes: _______________

## M4 — Retry creates a new attempt

**Objective:** Manual Retry starts a fresh attempt: % resets to 0 exactly once, then climbs monotonically; stale events from the old attempt never overwrite the new one.

**Given:** App in `stalled` state from M3 (old attempt = A).
**When:** The tester presses Retry (manual button; no auto-retry exists).
**Then:** Pill returns to loading/downloading from 0% exactly once on the new attempt (A+1), then advances monotonically; supervisor consumes `attempt` and discards stale events from attempt A.
**Error Path:** If the new attempt also hangs, it re-enters `stalled` with a Retry offer again — never a silent freeze, never an automatic restart loop.
**UI State:** Pill back to loading/downloading from 0%; Retry button hides once progress resumes.
**OBS Behavior:** Silence until `ready`; no phantom subtitles during the retry.

**Steps:**
1. From the M3 `stalled` state, note the current `attempt` value (A) in logs.
2. Press the Retry button once.
3. Observe the pill reset to 0% / loading exactly once.
4. Verify subsequent % samples are monotonic on attempt A+1.
5. Confirm no log line from attempt A updates the pill after the retry.

**Evidence to record:**
- Screenshot of pill just after Retry (0% / loading) + one later sample (e.g. 15%).
- Log excerpt showing old `attempt: A` then new `attempt: A+1` and the single reset.
- Note confirming no stale-attempt overwrite.

- [ ] PASS / [ ] FAIL — Notes: _______________

## M5 — Actionable failure with code (offline)

**Objective:** Without internet and without a cached model, provisioning fails with an actionable `provision-*` code and one-line hint — no traceback in the UI, no phantom subtitles.

**Given:** No internet connectivity; model NOT in cache.
**When:** The app starts provisioning the `small` model.
**Then:** The pill shows the matching code's one-line ES/EN hint (expected `provision-network`: "Sin conexión. Revisa tu red y reintenta." / "No connection. Check your network and retry."); logs carry the code + sanitized exception class; `model-not-found` does NOT appear (reserved for real absence).
**Error Path:** Unknown exceptions map to `provision-unknown` with the sanitized class name only — never a raw traceback in the UI.
**UI State:** Error pill + one remediation line, localized ES/EN.
**OBS Behavior:** No partial or phantom subtitles for the failed provisioning run.

**Steps:**
1. Disconnect network (airplane mode / unplug / firewall block) and ensure the model is not cached.
2. Start the app and observe the ASR pill.
3. Record the exact pill text (ES and/or EN).
4. Check logs for the `provision-*` code + sanitized exception class, and confirm no traceback is rendered in the UI.
5. Confirm OBS overlay stays empty; then restore network.

**Evidence to record:**
- Screenshot of error pill with the one-line hint.
- Log excerpt with the code (expected `provision-network`) + sanitized class.
- OBS-empty confirmation (screenshot or note).

- [ ] PASS / [ ] FAIL — Notes: _______________

## M6 — Prewarm toggle persists

**Objective:** The prewarm GUI switch reads/writes the same config as `--prewarm/--lazy`, persists across restarts, and fresh installs still default to `true`.

**Given:** App closed; config file accessible.
**When:** The tester flips the prewarm switch in the Model tab, applies, closes, and reopens the app.
**Then:** The switch keeps the chosen value and it equals the `prewarm` key in config; a fresh install (or untouched config) still defaults to `true` with the approved ES/EN first-use download/network copy visible.
**Error Path:** If apply fails or the value is unreadable, the UI tells the truth about what was (not) saved instead of silently reverting.
**UI State:** Prewarm switch + explicit ES/EN copy about first-use download/network behavior; state survives restart.
**OBS Behavior:** Unchanged.

**Steps:**
1. With the app closed, note the current `prewarm` value in config.
2. Open the app → Model tab → flip the prewarm switch → Apply.
3. Close and reopen the app; verify the switch position is unchanged.
4. Verify the config `prewarm` key equals the switch position.
5. (Fresh-default check) Confirm default `true` on a fresh profile/install or document the untouched-config value.

**Evidence to record:**
- Before/after screenshots of the switch + the approved copy text.
- Config before/after snippet (`prewarm: true/false`).
- Note of the fresh-install default.

- [ ] PASS / [ ] FAIL — Notes: _______________

## M7 — Legacy OpenCohost compat via --health-file

**Objective:** Service mode health snapshots carry both the honest `asr_state` and the legacy mirror collapsed to `loading/ready/failed`.

**Given:** Service mode launched with `--health-file <path>`.
**When:** Provisioning moves through `downloading/loading/transcribing` (and later `ready`, or `stalled`/`failed` if forced).
**Then:** Each snapshot contains honest `asr_state` AND `asr_state_legacy` where honest `downloading/loading/transcribing/stalled` → `loading`, `ready` → `ready`, `failed` → `failed`.
**Error Path:** A missing or unwritable health file is reported plainly and never blocks provisioning or crashes the supervisor.
**UI State:** Unchanged (health file is the contract surface, not the pill).
**OBS Behavior:** Overlay unchanged.

**Steps:**
1. IMPORTANT: revert the M3 `startup_stall_sec: 5` override to default first.
2. Launch in service mode with `--health-file <path>` (record the exact command).
3. Capture the health snapshot during provisioning (expect honest `downloading`/`loading` + legacy `loading`).
4. Capture a second snapshot at/after `ready` (expect `ready` + `ready`).
5. (Optional) Force `stalled`/`failed` and confirm the legacy collapse still holds.

**Evidence to record:**
- Exact `--health-file` command used.
- Two (or more) snapshot excerpts showing `asr_state` + `asr_state_legacy` side by side.
- Note of any collapse mismatch (FAIL if any honest state maps outside the contract).

- [ ] PASS / [ ] FAIL — Notes: _______________

## M8 — Recovery without burst (backlog cap 256)

**Objective:** After M3+M4 recovery under backlog `auto`, live-edge subtitles resume with at most 256 catch-up items in `send_all` — no history dump.

**Given:** M3+M4 recovery path completed with backlog policy `auto`.
**When:** Provisioning returns to `ready` and transcription resumes.
**Then:** Subtitles continue at the live edge; catch-up in `send_all` mode is capped at 256; no full-history burst floods OBS.
**Error Path:** If backlog state is corrupt or oversized, the cap still holds and the overflow is dropped/skipped per policy — never dumped into the overlay.
**UI State:** Pill back to `ready`/transcribing; no error residue.
**OBS Behavior:** Normal flow resumes; overlay shows live-edge subtitles only, no burst volley.

**Steps:**
1. Set backlog policy `auto` (record the exact key/value).
2. Replay the M3 (stall) → M4 (retry) path while OBS Browser Source is visible.
3. Let provisioning reach `ready` and speak/test transcription.
4. Count or estimate catch-up subtitles delivered on recovery (must be ≤ 256 in `send_all`).
5. Confirm OBS shows continuous normal flow with no burst volley.

**Evidence to record:**
- Backlog config snippet (`auto`, `send_all` cap 256).
- OBS observation note + recovery timestamps.
- Catch-up count or counter/log excerpt proving ≤ 256.

- [ ] PASS / [ ] FAIL — Notes: _______________

## Closure — how to package evidence per ticket (no commits)

1. For each M-ticket, collect: pill screenshot(s) + OBS state note/screenshot + the cited log/health/config excerpt. Keep filenames ticket-scoped, e.g. `M1-pill-38pct.png`, `M3-stalled-retry.png`, `M5-provision-network-log.txt`, `M7-health-downloading.json`.
2. Fill the PASS/FAIL checkbox and the Notes line in this sheet for every ticket; a ticket with no recorded evidence counts as NOT validated ([Agent QA Qwen 3.6Plus] decision rule).
3. Do NOT commit, push, merge, or revert anything to hand over evidence — share the files/folder out-of-band (e.g. attach to the track review or PO handoff) and leave the working tree untouched.
4. Revert checklist before handoff: temporary `startup_stall_sec: 5` removed (M3), network restored (M5), model cache state documented (renamed/restored or not), prewarm value documented (M6).
5. Validate handoff read-only: `git status --short --branch` (expect only the pre-existing dirty/untracked entries plus this new sheet) and re-read this file to confirm all 8 checkboxes are marked.
6. Traceability for the reviewer: criteria derive from `spec.md` T1–T4 blocks + `tasks.md:40-66`; wire rules (`percent` 0–100 float, monotonic per `attempt`, single reset to 0, `asr_state_legacy` collapse, `provision-*` catalog) are the pass/fail oracle when a pill text is ambiguous.

## Skill resolution

- Paths injected (read before work): `E:\LiveAudio\.agents\skills\conductor-status\SKILL.md`, `E:\LiveAudio\.agents\skills\liveaudio-qa-qwen36plus\SKILL.md`, `E:\LiveAudio\.agents\skills\liveaudio-research-gemini25pro\SKILL.md`.
