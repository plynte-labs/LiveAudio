// SPDX-License-Identifier: MIT
/**
 * Static and runtime tests for ribbon rendering, adaptive state machine,
 * WebSocket hello handshake, visibility lifecycle, and backpressure safety.
 *
 * Ported from:
 *   - tests/test_ribbon_js.py
 *   - tests/test_ribbon_runtime.py
 *   - tests/test_overlay_hello.py
 *   - tests/test_overlay_visibility.py
 *   - tests/test_backpressure_js.py
 */

import fs from 'node:fs';
import path from 'node:path';
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { runScenario } from './ribbon_harness.cjs';

const htmlPath = path.resolve(import.meta.dirname, '..', 'assets', 'subtitulos_obs.html');
const htmlContent = fs.readFileSync(htmlPath, 'utf8');

// =============================================================================
// Static Analysis Suites (from test_ribbon_js.py & test_backpressure_js.py)
// =============================================================================

describe('Static: Mode and Line Parameter Parsing', () => {
  it('parses ?mode= URL parameter via urlParams.get("mode")', () => {
    assert.match(
      htmlContent,
      /urlParams\.get\(\s*['"]mode['"]\s*\)/,
      'Overlay must parse ?mode= from the URL via urlParams.get("mode")'
    );
  });

  it('parses ?lines= URL parameter via urlParams.get("lines")', () => {
    assert.match(
      htmlContent,
      /urlParams\.get\(\s*['"]lines['"]\s*\)/,
      'Overlay must parse ?lines= from the URL via urlParams.get("lines")'
    );
  });

  it('contains valid display modes set: single, ribbon, adaptive', () => {
    for (const mode of ['single', 'ribbon', 'adaptive']) {
      assert.ok(htmlContent.includes(`'${mode}'`), `Client must recognize '${mode}' mode`);
    }
  });

  it('defaults resolvedMode to "adaptive"', () => {
    assert.match(
      htmlContent,
      /resolvedMode\s*=/,
      'A resolvedMode variable must hold the resolved render mode'
    );
    assert.match(
      htmlContent,
      /resolvedMode\s*=[^;]*['"]adaptive['"]/,
      'resolvedMode must default to "adaptive" when ?mode= is absent or invalid'
    );
  });

  it('reads resolvedMode in the render path decision (anti-dead-variable guard)', () => {
    const enqueueMatch = htmlContent.match(/function enqueueSubtitle\([^)]*\)\s*\{(.*?)\n {8}\}/s);
    assert.ok(enqueueMatch, 'enqueueSubtitle function body not found');
    assert.ok(
      enqueueMatch[1].includes('resolveRenderStacked'),
      'enqueueSubtitle must invoke resolveRenderStacked'
    );

    const decisionMatch = htmlContent.match(/function resolveRenderStacked\([^)]*\)\s*\{(.*?)\n {8}\}/s);
    assert.ok(decisionMatch, 'resolveRenderStacked function body not found');
    assert.ok(
      decisionMatch[1].includes('resolvedMode'),
      'resolveRenderStacked must read resolvedMode'
    );
  });

  it('clamps ?lines= client-side between 1 and 8', () => {
    assert.match(
      htmlContent,
      /RIBBON_MAX_LINES_CEILING\s*=\s*8/,
      'Ceiling constant RIBBON_MAX_LINES_CEILING must be 8'
    );
    assert.match(
      htmlContent,
      /RIBBON_MIN_LINES\s*=\s*1/,
      'Floor constant RIBBON_MIN_LINES must be 1'
    );
    assert.match(
      htmlContent,
      /Math\.max\(\s*RIBBON_MIN_LINES\s*,\s*Math\.min\(\s*RIBBON_MAX_LINES_CEILING\s*,/,
      '?lines= must clamp via Math.max(MIN, Math.min(CEILING, n))'
    );
    assert.match(
      htmlContent,
      /ribbonMaxLines/,
      'A ribbonMaxLines variable must hold the resolved visible cap'
    );
  });

  it('reads ribbonMaxLines in the eviction render path', () => {
    const occurrences = (htmlContent.match(/ribbonMaxLines/g) || []).length;
    assert.ok(
      occurrences >= 2,
      'ribbonMaxLines must be both defined and read in the render path'
    );
  });
});

describe('Static: Single Mode Legacy Path Restoration', () => {
  it('clears shared hide and cleanup timers in single path', () => {
    assert.ok(htmlContent.includes('clearTimeout(hideTimeout)'));
    assert.ok(htmlContent.includes('clearTimeout(cleanupTimeout)'));
  });

  it('removes prior box before appending new one in single path', () => {
    assert.match(
      htmlContent,
      /container\.removeChild\(existing\)/,
      'Single path must remove the prior .sub-box before appending the new one'
    );
  });

  it('nullifies existing reference for garbage collection in single path', () => {
    assert.match(htmlContent, /existing\s*=\s*null/);
  });

  it('gates the shared timer and removeChild block with !renderStacked', () => {
    assert.ok(
      htmlContent.includes('renderStacked'),
      'showSubtitle must take a renderStacked flag to branch single vs stacked'
    );
    assert.match(
      htmlContent,
      /if\s*\(\s*!\s*renderStacked\s*\)/,
      'The legacy shared-timer/removeChild block must be gated by !renderStacked'
    );
  });

  it('serializes single mode through isShowing and onSubtitleComplete', () => {
    assert.ok(htmlContent.includes('isShowing'));
    assert.ok(htmlContent.includes('onSubtitleComplete'));
  });
});

describe('Static: Ribbon Render Branch and Per-Line Timers', () => {
  it('stores per-line _hideTimer on stacked nodes', () => {
    assert.match(
      htmlContent,
      /\bsub\._hideTimer\s*=/,
      'Stacked boxes must store their own _hideTimer on the node'
    );
  });

  it('stores per-line _cleanupTimer on stacked nodes', () => {
    assert.match(
      htmlContent,
      /\bsub\._cleanupTimer\s*=/,
      'Stacked boxes must store their own _cleanupTimer on the node'
    );
  });

  it('records _bornAt timestamp on nodes for lifetime adoption', () => {
    assert.match(
      htmlContent,
      /\bsub\._bornAt\s*=/,
      'Each box must record _bornAt = Date.now() for adoption math'
    );
  });

  it('nulls timer ids on stacked cleanup without pumping serialized queue', () => {
    assert.match(
      htmlContent,
      /sub\._hideTimer\s*=\s*sub\._cleanupTimer\s*=\s*null/,
      'Stacked per-line cleanup must null both per-line timer ids'
    );
  });

  it('targets oldest box via firstElementChild during cap eviction', () => {
    const evictMatch = htmlContent.match(
      /while\s*\(\s*container\.children\.length\s*>=\s*ribbonMaxLines\s*\)\s*\{(.*?)\n {8}\}/s
    );
    assert.ok(evictMatch, 'Cap-eviction while-loop not found');
    const body = evictMatch[1];
    assert.ok(
      body.includes('container.firstElementChild'),
      'Cap eviction must evict the OLDEST box via container.firstElementChild'
    );
    assert.ok(
      !body.includes('container.lastElementChild'),
      'Cap eviction must NOT target lastElementChild (that is the newest box)'
    );
  });

  it('clears timers on evicted box before calling removeChild', () => {
    assert.match(
      htmlContent,
      /clearTimeout\(\s*\w+\._hideTimer\s*\)/,
      'Eviction must clearTimeout(box._hideTimer)'
    );
    assert.match(
      htmlContent,
      /clearTimeout\(\s*\w+\._cleanupTimer\s*\)/,
      'Eviction must clearTimeout(box._cleanupTimer)'
    );
  });

  it('governs cap eviction loop by ribbonMaxLines', () => {
    assert.match(
      htmlContent,
      /children\.length\s*>=\s*ribbonMaxLines/,
      'Cap eviction must compare children count against ribbonMaxLines'
    );
  });

  it('clears per-line timers if single-path removes a ribbon survivor box', () => {
    const blockMatch = htmlContent.match(/if\s*\(existing\)\s*\{(.*?)\}/s);
    assert.ok(blockMatch, 'Single-path removeChild block not found');
    assert.ok(
      blockMatch[1].includes('_hideTimer'),
      'Single-path removeChild must clear the removed node per-line timers'
    );
  });
});

describe('Static: Ribbon Stacking CSS and Layout Direction', () => {
  it('applies flex-direction: column-reverse on ribbon-active container', () => {
    assert.match(
      htmlContent,
      /flex-direction:\s*column-reverse/,
      'Ribbon container must use flex-direction: column-reverse (newest on top)'
    );
  });

  it('toggles ribbon-active class from JS with state changes', () => {
    assert.match(
      htmlContent,
      /classList\.(add|remove|toggle)\(\s*['"]ribbon-active['"]/,
      'JS must toggle ribbon-active container class'
    );
  });

  it('activates column-reverse class for pinned ribbon mode', () => {
    const decisionMatch = htmlContent.match(/function resolveRenderStacked\([^)]*\)\s*\{(.*?)\n {8}\}/s);
    assert.ok(decisionMatch, 'resolveRenderStacked function body not found');
    const ribbonBranchMatch = decisionMatch[1].match(/resolvedMode\s*===\s*['"]ribbon['"]\s*\)\s*\{(.*?)\}/s);
    assert.ok(ribbonBranchMatch, 'Ribbon-mode branch not found in decision');
    assert.ok(
      ribbonBranchMatch[1].includes('setRibbonActive(true)'),
      'Pinned ribbon mode must activate the column-reverse container class'
    );
  });

  it('preserves exit animation .sub-box.hiding styling', () => {
    assert.match(
      htmlContent,
      /\.sub-box\.hiding\s*\{[^}]*opacity:\s*0\s*!important/,
      '.sub-box.hiding exit CSS must be preserved'
    );
  });

  it('preserves 50px bottom anchor margin', () => {
    assert.match(htmlContent, /margin-bottom:\s*50px/);
  });
});

describe('Static: Adaptive State Machine Rules', () => {
  it('defines module-level adaptiveState variable', () => {
    assert.match(
      htmlContent,
      /adaptiveState\s*=/,
      'Adaptive machine must have an adaptiveState variable'
    );
  });

  it('promotes on pendingQueue length > 1 OR isReplay', () => {
    assert.match(
      htmlContent,
      /pendingQueue\.length\s*>\s*1/,
      'Promotion must be edge-triggered on pendingQueue.length > 1'
    );
    assert.match(
      htmlContent,
      /\bisReplay\b/,
      'Promotion must also consider the isReplay payload signal'
    );
  });

  it('threads is_replay signal from onmessage to enqueueSubtitle', () => {
    assert.match(
      htmlContent,
      /data\.is_replay/,
      'onmessage must read data.is_replay'
    );
    assert.match(
      htmlContent,
      /function enqueueSubtitle\(\s*text\s*,\s*style\s*,\s*isReplay/,
      'enqueueSubtitle must accept isReplay parameter'
    );
  });

  it('requires drain to <= 1 pending and settle window via DEBOUNCE_MS for demotion', () => {
    assert.match(
      htmlContent,
      /pendingQueue\.length\s*<=\s*1/,
      'Demotion gate must require pendingQueue drained to <= 1'
    );
    assert.ok(
      htmlContent.includes('DEBOUNCE_MS'),
      'Demotion settle window must reuse DEBOUNCE_MS'
    );
  });

  it('includes replayActive latch to prevent mid-replay demotion', () => {
    assert.match(
      htmlContent,
      /replayActive/,
      'A replay latch variable must gate demotion while replay is active'
    );
  });

  it('gates the state machine behind resolvedMode === "adaptive"', () => {
    assert.match(
      htmlContent,
      /resolvedMode\s*===\s*['"]adaptive['"]/,
      'The state machine must only run when resolvedMode === "adaptive"'
    );
    assert.match(
      htmlContent,
      /resolvedMode\s*===\s*['"]ribbon['"]/,
      'Ribbon mode must pin the ribbon render branch'
    );
  });

  it('adopts live box remaining lifetime upon promotion', () => {
    assert.match(
      htmlContent,
      /SUB_LIFETIME_MS\s*=\s*5000/,
      'SUB_LIFETIME_MS must be 5000'
    );
    assert.match(
      htmlContent,
      /Math\.max\(\s*0\s*,\s*subLifetimeMs\s*-\s*\(\s*Date\.now\(\)\s*-\s*bornAt\s*\)/,
      'Adoption must re-arm for remaining lifetime'
    );

    const adoptMatch = htmlContent.match(/function adoptBoxIntoRibbon\([^)]*\)\s*\{(.*?)\n {8}\}/s);
    assert.ok(adoptMatch, 'adoptBoxIntoRibbon function body not found');
    assert.match(
      adoptMatch[1],
      /armPerLineExpiry\(\s*box\s*,\s*remaining\s*\)/,
      'Adoption must arm the per-line timer with remaining lifetime'
    );
  });

  it('resets isShowing = false on promotion to rearm single path post-demote', () => {
    const decisionMatch = htmlContent.match(/function resolveRenderStacked\([^)]*\)\s*\{(.*?)\n {8}\}/s);
    assert.ok(decisionMatch);
    const promoteMatch = decisionMatch[1].match(/adaptiveState\s*=\s*['"]RIBBON['"]\s*;(.*?)\n {12}\}/s);
    assert.ok(promoteMatch, 'Promotion branch not found in resolveRenderStacked');
    assert.match(
      promoteMatch[1],
      /isShowing\s*=\s*false/,
      'Promotion must reset isShowing = false'
    );
  });

  it('ages out replayActive latch on drained silence in demote check', () => {
    const schedMatch = htmlContent.match(/function scheduleDemoteCheck\([^)]*\)\s*\{(.*?)\n {8}\}/s);
    assert.ok(schedMatch, 'scheduleDemoteCheck function body not found');
    assert.match(
      schedMatch[1],
      /replayActive\s*=\s*false/,
      'The demote settle callback must clear replayActive = false'
    );
  });

  it('bounds ribbon enqueue with MAX_PENDING_QUEUE to avoid memory leaks during bursts', () => {
    const enqueueMatch = htmlContent.match(/function enqueueSubtitle\([^)]*\)\s*\{(.*?)\n {8}\}/s);
    assert.ok(enqueueMatch);
    const ribbonBranchMatch = enqueueMatch[1].match(/if\s*\(\s*renderStacked\s*\)\s*\{(.*?)\n {12}\}/s);
    assert.ok(ribbonBranchMatch, 'Ribbon enqueue branch not found');
    const branchBody = ribbonBranchMatch[1];
    assert.match(
      branchBody,
      /pendingQueue\.length\s*>=\s*MAX_PENDING_QUEUE/,
      'Ribbon enqueue must guard against MAX_PENDING_QUEUE before push'
    );
    assert.ok(
      branchBody.includes('pendingQueue.shift()'),
      'Ribbon enqueue must discard oldest pending item on overflow'
    );
  });
});

describe('Static: FLIP Web Animations Cascade Preconditions', () => {
  it('does not set !important on .sub-box.visible transform', () => {
    const blockMatch = htmlContent.match(/\.sub-box\.visible\s*\{(.*?)\}/s);
    assert.ok(blockMatch, '.sub-box.visible rule not found');
    const transformLine = blockMatch[1].match(/transform:[^;]*;/);
    assert.ok(transformLine, '.sub-box.visible must set a transform');
    assert.ok(
      !transformLine[0].includes('!important'),
      'Transform on .sub-box.visible must NOT be !important or FLIP will be suppressed'
    );
  });

  it('uses Web Animations API element.animate for survivor glide', () => {
    assert.ok(htmlContent.includes('flipPlayRibbon'), 'FLIP play helper must exist');
    assert.ok(
      htmlContent.includes('.animate('),
      'FLIP must drive glide via element.animate()'
    );
  });
});

describe('Static: Backpressure and Queue Safety', () => {
  it('defines pacing debounce DEBOUNCE_MS = 80', () => {
    assert.match(
      htmlContent,
      /const\s+DEBOUNCE_MS\s*=\s*80/,
      'DEBOUNCE_MS = 80 pacing must be preserved'
    );
  });

  it('defines MAX_PENDING_QUEUE = 5 as transport safety valve', () => {
    assert.match(
      htmlContent,
      /const\s+MAX_PENDING_QUEUE\s*=\s*5/,
      'MAX_PENDING_QUEUE must stay as the transport safety valve'
    );
  });

  it('maintains pendingQueue.shift() for oldest packet discard', () => {
    assert.ok(
      htmlContent.includes('pendingQueue.shift()'),
      'Single-mode discard-oldest backpressure must be preserved'
    );
  });

  it('decouples visible depth governed by ribbonMaxLines from transport queue', () => {
    assert.ok(
      htmlContent.includes('ribbonMaxLines'),
      'Visible cap must be governed by ribbonMaxLines, not MAX_PENDING_QUEUE'
    );
  });

  it('renders ribbon path via renderStacked without waiting on isShowing gate', () => {
    assert.ok(
      htmlContent.includes('renderStacked'),
      'Ribbon rendering must use a renderStacked path, not the isShowing gate'
    );
  });
});

// =============================================================================
// Runtime Scenarios (from test_ribbon_runtime.py, test_overlay_hello.py, test_overlay_visibility.py)
// =============================================================================

describe('Runtime: Adaptive State Machine Transitions (test_ribbon_runtime.py)', () => {
  it('scenario1: promotes on burst, drains, demotes, and renders spaced single line', () => {
    const result = runScenario('scenario1');
    assert.equal(result.promotedRibbon, true, 'Fast burst must promote to RIBBON');
    assert.equal(result.demotedToSingle, true, 'Drained ribbon must demote back to SINGLE');
    assert.equal(result.emptyAfterDrain, 0, 'Ribbon must be empty after the drain window');
    assert.equal(
      result.singleRendered,
      1,
      'A spaced single line after demotion MUST render exactly one .sub-box'
    );
    assert.deepEqual(result.singleText, ['lonely']);
  });

  it('scenario2: evicts oldest box (firstElementChild) when cap is exceeded', () => {
    const result = runScenario('scenario2');
    assert.equal(result.liveCount, result.cap, 'Live boxes must equal cap after overflow');
    assert.equal(result.oldestEvicted, true, 'The oldest boxes (L0, L1) must be evicted');
    assert.equal(result.newestKept, true, 'The newest box (L4) must survive');
    assert.deepEqual(
      result.liveTexts,
      ['L2', 'L3', 'L4'],
      'Survivors must be the three newest lines in insertion order'
    );
  });

  it('scenario3: demotes cleanly after replay burst is followed by silence', () => {
    const result = runScenario('scenario3');
    assert.equal(result.ribbonDuringReplay, true, 'An is_replay burst must promote to RIBBON');
    assert.equal(
      result.demotedAfterSilence,
      true,
      'replayActive latch must age out on silence so demotion completes'
    );
  });

  it('scenario4: spaced replay does not flap mid-stream and demotes once idle', () => {
    const result = runScenario('scenario4');
    assert.equal(result.ribbonDuringReplay, true, 'Spaced replay must promote to RIBBON');
    assert.equal(
      result.demotedMidStream,
      false,
      'The ribbon must not toggle off at any point while spaced replay is ongoing'
    );
    assert.equal(
      result.demotedAfterIdle,
      true,
      'After replay falls silent past idle window, demotion must complete'
    );
  });

  it('scenario5: FLIP reposition glides surviving lines and lets new line enter on its own', () => {
    const result = runScenario('scenario5');
    assert.equal(result.promoted, true, 'Fast burst must promote to RIBBON');
    assert.deepEqual(result.beforeTexts, ['L0', 'L1', 'L2']);
    assert.equal(result.stillCapped, true, 'Ribbon must stay at cap after new line');
    assert.deepEqual(result.afterTexts, ['L1', 'L2', 'L3']);
    assert.equal(
      result.survivorsGlided,
      true,
      'Surviving ribbon lines must be FLIP-animated so they glide'
    );
    assert.equal(
      result.newLineNotFlipped,
      true,
      'The newly added line must keep its own enter animation'
    );
  });
});

describe('Runtime: WebSocket Hello Handshake and Reconnect (test_overlay_hello.py)', () => {
  it('discards frames until valid hello packet is received (helloGate)', () => {
    const result = runScenario('helloGate');
    assert.equal(result.beforeHello, 0, 'Frames before hello must be discarded');
    assert.deepEqual(result.afterHello, ['accepted'], 'Frames after hello must be accepted');
  });

  it('closes socket on hello timeout and advances port (helloTimeout)', () => {
    const result = runScenario('helloTimeout');
    assert.equal(result.firstClosed, true, 'Foreign socket must be closed on timeout');
    assert.deepEqual(
      result.urls.slice(0, 2),
      ['ws://127.0.0.1:8765', 'ws://127.0.0.1:8766'],
      'Scan must advance to the next port upon timeout'
    );
  });

  it('rescans from base port on active disconnect (activeReconnect)', () => {
    const result = runScenario('activeReconnect');
    assert.deepEqual(
      result.urls.slice(0, 2),
      ['ws://127.0.0.1:8765', 'ws://127.0.0.1:8765'],
      'Reconnect must rescan starting from wsBasePort'
    );
  });

  it('prevents stale socket callbacks from mutating current connection (staleCallbacks)', () => {
    const result = runScenario('staleCallbacks');
    assert.equal(result.before, 2);
    assert.equal(result.after, 2);
    assert.deepEqual(result.rendered, [], 'Stale messages must not render');
  });

  it('caps full scan backoff at 8000ms (backoffCap)', () => {
    const result = runScenario('backoffCap');
    assert.deepEqual(result.backoffs, [1000, 2000, 4000, 8000, 8000]);
  });
});

describe('Runtime: Visibility Lifecycle (test_overlay_visibility.py)', () => {
  it('reveals transition-based styles without depending on requestAnimationFrame (revealWithoutRaf)', () => {
    const result = runScenario('revealWithoutRaf');
    for (const [style, revealed] of Object.entries(result.revealed)) {
      assert.ok(
        revealed,
        `Style '${style}' must gain .visible while document is hidden`
      );
    }
    assert.equal(
      result.rafPending,
      0,
      'The reveal path must not enqueue requestAnimationFrame callback'
    );
  });

  it('hard-resets when becoming visible, dropping hidden frames without flash (visibilityReset)', () => {
    const result = runScenario('visibilityReset');
    assert.ok(result.liveWhileHidden, 'Precondition: overlay renders while hidden');
    assert.deepEqual(
      result.afterUnhide,
      [],
      'No subtitle produced while scene was inactive may survive unhide'
    );
    assert.equal(
      result.staleDroppedSilently,
      true,
      'Stale boxes must be detached without being revealed or animated'
    );
    assert.equal(
      result.ribbonAfterUnhide,
      false,
      'Ribbon active layout must be reset on resume'
    );
    assert.deepEqual(
      result.freshTexts,
      ['fresh'],
      'A subtitle arriving after unhide must render normally'
    );
    assert.equal(
      result.freshVisible,
      true,
      'Post-resume subtitle must become visible'
    );
  });
});

describe('Runtime: provisional partial subtitles', () => {
  it('renders the partial in the final slot and replaces it in place (partialSubtitle)', () => {
    const r = runScenario('partialSubtitle');
    assert.deepEqual(r.first, ['hel']);
    assert.deepEqual(r.overwritten, ['hello wor'], 'each partial overwrites the previous one');
    assert.equal(r.reusedNode, true, 'updates reuse the same box');
    assert.deepEqual(r.afterIdle, ['hello wor'], 'a partial never enters the final queue');
    assert.equal(r.finalReplacedNode, true, 'the final replaces the provisional box');
    assert.equal(r.finalVisibleAtOnce, true, 'the replacing final is visible immediately (no fade-in flicker)');
    assert.deepEqual(r.finals, ['hello world'], 'one box holds the final');
    assert.deepEqual(r.takeover, ['next'], 'a new partial takes over the slot of the previous final');
    assert.deepEqual(r.clearedByOwnFinal, ['next one']);
    assert.deepEqual(r.clearedByEmpty, [], 'empty partial text clears the line');
    assert.deepEqual(r.clearedOnResume, [], 'resume drops the provisional line');
  });

  it('inserts the partial where the next ribbon line goes and the final takes its place (partialRibbon)', () => {
    const r = runScenario('partialRibbon');
    assert.equal(r.ribbon, true);
    assert.deepEqual(r.withPartial, ['A', 'B', 'par'], 'partial is the newest ribbon line');
    assert.deepEqual(r.updated, ['A', 'B', 'partial']);
    assert.deepEqual(r.afterFinal, ['A', 'B', 'final'], 'final takes the partial line position');
    assert.deepEqual(r.clearedByEmpty, ['A', 'B', 'final']);
  });

  it('has no separate partial container above the finals', () => {
    assert.doesNotMatch(htmlContent, /partial-container/);
  });

  it('keys partial frames on partial_text so overlays that predate them ignore the frame', () => {
    assert.match(htmlContent, /data\.type === 'subtitle_partial'/);
    assert.doesNotMatch(htmlContent, /data\.partial_text\s*\)\s*\{\s*enqueue/);
  });
});
