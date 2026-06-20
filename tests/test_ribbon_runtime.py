# SPDX-License-Identifier: MIT
"""Behavioral runtime coverage for the adaptive SINGLE<->RIBBON state machine.

Unlike tests/test_ribbon_js.py (static regex on the HTML), these tests EXECUTE
the overlay's real <script> from liveaudio/assets/subtitulos_obs.html under Node
via tests/ribbon_harness.js, which supplies a minimal synchronous DOM/timer/
WebSocket shim and a virtual clock. Each scenario runs in its OWN Node process
(fresh module state) and emits a JSON result line.

These pin the four confirmed runtime defects so they FAIL ON REVERT:
  scenario1 -> FIX 1: promote->drain->demote->spaced single line must still render
                      (orphaned isShowing would swallow the post-demote single line).
  scenario2 -> FIX 2: cap overflow must evict the OLDEST box (firstElementChild),
                      not the newest (lastElementChild flipped only visually).
  scenario3 -> FIX 3: a replay burst followed by SILENCE must demote (the
                      replayActive latch must age out, else stuck in RIBBON forever).
  scenario4 -> FIX 3b: SPACED replay (gap > DEBOUNCE_MS, the normal catch-up
                      cadence) must NOT flap RIBBON->SINGLE->RIBBON mid-stream;
                      the latch clear must be gated on genuine replay-IDLE, and
                      demotion must still complete once replay is silent past the
                      idle window. An unconditional latch clear reintroduces the flap.

If Node is unavailable the whole module SKIPS so CI without Node still passes.
"""

import json
import os
import shutil
import subprocess

import pytest

_HARNESS = os.path.join(os.path.dirname(__file__), "ribbon_harness.js")
_NODE = shutil.which("node")

pytestmark = pytest.mark.skipif(
    _NODE is None,
    reason="Node.js not available; JS-execution harness for the overlay is skipped",
)


def _run_scenario(name):
    """Run one harness scenario in a fresh Node process and parse its JSON."""
    proc = subprocess.run(
        [_NODE, _HARNESS, name],
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert proc.returncode == 0, (
        f"harness scenario {name} failed (rc={proc.returncode}):\n"
        f"STDOUT: {proc.stdout}\nSTDERR: {proc.stderr}"
    )
    assert proc.stdout.strip(), f"harness scenario {name} produced no output"
    return json.loads(proc.stdout)


def test_promote_drain_demote_then_single_line_renders():
    """FIX 1 / AC-15 / AC-16: after promote->drain->demote, a spaced single line renders.

    A fast burst promotes to RIBBON; draining empties the ribbon and the settle
    window demotes to SINGLE. A later spaced single line MUST render via the single
    path. With isShowing orphaned at true (the bug), onSubtitleComplete never runs
    for adopted boxes, the single guard `if (!isShowing) processQueue()` is dead, and
    the line is swallowed (singleRendered == 0).
    """
    r = _run_scenario("scenario1")
    assert r["promotedRibbon"] is True, "fast burst must promote to RIBBON"
    assert r["demotedToSingle"] is True, "drained ribbon must demote back to SINGLE"
    assert r["emptyAfterDrain"] == 0, "ribbon must be empty after the drain window"
    assert r["singleRendered"] == 1, (
        "a spaced single line after demotion MUST render exactly one .sub-box "
        "(orphaned isShowing would swallow it)"
    )
    assert r["singleText"] == ["lonely"]


def test_cap_overflow_evicts_oldest_box():
    """FIX 2 / AC-4: cap overflow evicts the OLDEST box, keeping the newest.

    With ribbonMaxLines=3 and five lines L0..L4, the surviving boxes must be the
    three NEWEST (L2, L3, L4); L0 and L1 (oldest) must be evicted. Evicting
    lastElementChild (the bug) would remove the newest and keep the oldest.
    """
    r = _run_scenario("scenario2")
    assert r["liveCount"] == r["cap"], "live boxes must equal the cap after overflow"
    assert r["oldestEvicted"] is True, "the OLDEST boxes (L0, L1) must be evicted"
    assert r["newestKept"] is True, "the NEWEST box (L4) must survive"
    assert r["liveTexts"] == ["L2", "L3", "L4"], (
        "survivors must be the three newest lines in insertion order"
    )


def test_replay_burst_then_silence_demotes():
    """FIX 3: a replay burst followed by SILENCE must demote cleanly.

    is_replay payloads set replayActive=true. After the burst stops and no further
    traffic arrives, the settle callback must age out replayActive so demotion can
    complete. Without the fix the latch never clears and the overlay is stuck in
    RIBBON forever (demotedAfterSilence == false).
    """
    r = _run_scenario("scenario3")
    assert r["ribbonDuringReplay"] is True, "an is_replay burst must promote to RIBBON"
    assert r["demotedAfterSilence"] is True, (
        "after a replay burst ends in silence, the replayActive latch must age out "
        "so demotion completes (else stuck in RIBBON forever)"
    )


def test_spaced_replay_does_not_flap_then_demotes_after_idle():
    """FIX 3b: spaced replay must not flap the ribbon, then demote once idle.

    Catch-up replay payloads arrive spaced by catchup_interval_sec, which exceeds
    DEBOUNCE_MS (80ms). The settle timer fires in the gap between two replay payloads.
    With an UNCONDITIONAL latch clear (the regression) it clears replayActive, the
    demote guard passes, and the overlay collapses RIBBON->SINGLE mid-replay; the next
    payload re-promotes => a visible flap (collapse + reflow of the column-reverse
    stack). The idle-gated clear keeps the ribbon stable until replay has been silent
    past replayIdleMs, and demotion still completes after that idle window.
    """
    r = _run_scenario("scenario4")
    assert r["ribbonDuringReplay"] is True, "spaced replay must promote to RIBBON"
    assert r["demotedMidStream"] is False, (
        "the ribbon must NOT toggle off at any point while spaced replay is ongoing "
        "(an unconditional latch clear would flap RIBBON->SINGLE->RIBBON); "
        f"observed transitions: {r['transitions']}"
    )
    assert r["demotedAfterIdle"] is True, (
        "after replay falls silent past the idle window, the latch must age out so "
        "demotion completes (no wedge in RIBBON)"
    )


def test_flip_reposition_glides_survivors_not_new_line():
    """FLIP smoothing: surviving ribbon lines glide; the new line is not FLIP-animated.

    When the stacked ribbon reflows (oldest evicted + new line appended), the
    surviving lines must be animated from their old position to the new one via the
    Web Animations API (element.animate) so they glide instead of jumping. The
    just-added line must keep its OWN enter animation and not be FLIP-animated. This
    is purely visual: the cap and the per-line lifetime stay unchanged.
    """
    r = _run_scenario("scenario5")
    assert r["promoted"] is True, "fast burst must promote to RIBBON"
    assert r["beforeTexts"] == ["L0", "L1", "L2"], "ribbon must fill to its cap first"
    assert r["stillCapped"] is True, "the ribbon must stay at its cap after the new line"
    assert r["afterTexts"] == ["L1", "L2", "L3"], "oldest evicted, newest appended"
    assert r["survivorsGlided"] is True, (
        "surviving ribbon lines must be FLIP-animated (element.animate) so they glide "
        "to their new positions instead of jumping when the stack reflows"
    )
    assert r["newLineNotFlipped"] is True, (
        "the newly added line must NOT be FLIP-animated; it keeps its own enter animation"
    )
