// SPDX-License-Identifier: MIT
//
// Minimal headless harness that EXECUTES the real <script> from
// liveaudio/assets/subtitulos_obs.html under Node, driving the adaptive
// SINGLE<->RIBBON state machine through one behavioral scenario per process.
//
// Usage:  node ribbon_harness.js <scenario>
//   scenario1  promote -> drain -> demote -> spaced single line  (FIX 1)
//   scenario2  cap overflow evicts the OLDEST box                 (FIX 2)
//   scenario3  replay burst then SILENCE demotes cleanly          (FIX 3)
//   scenario4  SPACED replay (gap > DEBOUNCE_MS) never flaps off,  (FIX 3b)
//              then demotes once replay has been idle past the window
//
// Each scenario runs in its OWN process with a FRESH evaluation of the overlay
// script, so module-level state (adaptiveState, replayActive, lastShowTime,
// pendingQueue, isShowing) cannot leak across scenarios.
//
// A tiny synchronous DOM/timer/WebSocket shim (NOT jsdom) lets the overlay's own
// enqueue/pump/promote/demote/evict logic run verbatim. Timers are virtualized:
// advancing a logical clock flushes due callbacks so we can fast-forward the 80ms
// debounce, 5000ms hide and 650ms cleanup deterministically.
//
// Output: a single JSON line on stdout, consumed by tests/test_ribbon_runtime.py.

'use strict';

const fs = require('fs');
const path = require('path');

// ---- Virtual clock + timer queue -----------------------------------------
let now = 0;
let timerSeq = 1;
const timers = new Map(); // id -> { at, cb }
const scheduledDelays = [];

function setTimeoutShim(cb, delay) {
  scheduledDelays.push(delay || 0);
  const id = timerSeq++;
  timers.set(id, { at: now + (delay || 0), cb });
  return id;
}
function clearTimeoutShim(id) {
  if (id != null) timers.delete(id);
}
// Advance the virtual clock by `ms`, firing every due timer in time order.
function advance(ms) {
  const target = now + ms;
  while (true) {
    let nextId = null;
    let nextAt = Infinity;
    for (const [id, t] of timers) {
      if (t.at <= target && t.at < nextAt) {
        nextAt = t.at;
        nextId = id;
      }
    }
    if (nextId === null) break;
    const t = timers.get(nextId);
    timers.delete(nextId);
    now = t.at;
    t.cb();
  }
  now = target;
}

// ---- Minimal DOM shim -----------------------------------------------------
function matchesSelector(node, selector) {
  const cls = selector.replace(/^\./, '');
  return node._classes && node._classes.has(cls);
}

function createElement(tag) {
  const node = {
    tagName: String(tag).toUpperCase(),
    children: [],
    childNodes: [],
    parentNode: null,
    _classes: new Set(),
    _text: '',
    style: {
      _props: {},
      setProperty(k, v) { this._props[k] = v; },
    },
    appendChild(child) {
      child.parentNode = node;
      node.childNodes.push(child);
      if (child.tagName) node.children.push(child);
      return child;
    },
    removeChild(child) {
      const i = node.childNodes.indexOf(child);
      if (i >= 0) node.childNodes.splice(i, 1);
      const j = node.children.indexOf(child);
      if (j >= 0) node.children.splice(j, 1);
      child.parentNode = null;
      return child;
    },
    querySelector(sel) {
      return descendants(node).find((d) => matchesSelector(d, sel)) || null;
    },
    querySelectorAll(sel) {
      return descendants(node).filter((d) => matchesSelector(d, sel));
    },
    // Fake vertical layout so the overlay's FLIP can compute a delta: each line is
    // 50px tall and its top grows with DOM index. Eviction/append shift indices,
    // so survivors get a non-zero dy and the overlay calls .animate() on them.
    getBoundingClientRect() {
      const p = node.parentNode;
      const idx = p && p.children ? p.children.indexOf(node) : 0;
      return { top: idx * 50, bottom: idx * 50 + 40, left: 0, right: 0, width: 0, height: 40 };
    },
    // Web Animations API stub: records each FLIP animation so a scenario can assert
    // which lines glided. No timers involved -> never perturbs the virtual clock.
    animate(keyframes, opts) {
      node._animations = node._animations || [];
      node._animations.push({ keyframes, opts });
      return { finished: Promise.resolve(), cancel() {} };
    },
  };
  node.classList = {
    add(...cs) { cs.forEach((c) => node._classes.add(c)); },
    remove(...cs) { cs.forEach((c) => node._classes.delete(c)); },
    contains(c) { return node._classes.has(c); },
    toggle(c) { node._classes.has(c) ? node._classes.delete(c) : node._classes.add(c); },
  };
  Object.defineProperty(node, 'className', {
    get() { return [...node._classes].join(' '); },
    set(v) { node._classes = new Set(String(v).split(/\s+/).filter(Boolean)); },
  });
  Object.defineProperty(node, 'textContent', {
    get() { return node._text; },
    set(v) { node._text = String(v); node.childNodes = []; node.children = []; },
  });
  Object.defineProperty(node, 'firstElementChild', {
    get() { return node.children.length ? node.children[0] : null; },
  });
  Object.defineProperty(node, 'lastElementChild', {
    get() { return node.children.length ? node.children[node.children.length - 1] : null; },
  });
  return node;
}

function descendants(node) {
  const out = [];
  for (const c of node.childNodes) {
    if (c.tagName) {
      out.push(c);
      out.push(...descendants(c));
    }
  }
  return out;
}

const containerEl = createElement('div');
const documentElementEl = createElement('html');

const documentShim = {
  getElementById() { return containerEl; },
  createElement,
  createTextNode(t) { return { tagName: null, _text: String(t), parentNode: null }; },
  get documentElement() { return documentElementEl; },
};

// ---- WebSocket shim: captures the onmessage handler the overlay installs ---
let wsInstance = null;
const wsInstances = [];
class WebSocketShim {
  constructor(url) {
    this.url = url;
    this.onopen = null;
    this.onmessage = null;
    this.onclose = null;
    this.closed = false;
    wsInstance = this;
    wsInstances.push(this);
  }
  close() { this.closed = true; if (this.onclose) this.onclose(); }
}

function send(payload) {
  if (!wsInstance._identified) {
    if (wsInstance.onopen) wsInstance.onopen();
    wsInstance.onmessage({ data: JSON.stringify({ type: 'hello', app: 'liveaudio', proto: 1, port: 8765 }) });
    wsInstance._identified = true;
  }
  wsInstance.onmessage({ data: JSON.stringify(payload) });
}

// ---- Install globals + execute the real overlay script --------------------
global.document = documentShim;
global.window = { location: { search: '' } };
global.URLSearchParams = URLSearchParams;
global.WebSocket = WebSocketShim;
global.console = { log() {}, warn() {}, error() {} };
global.setTimeout = setTimeoutShim;
global.clearTimeout = clearTimeoutShim;
global.requestAnimationFrame = (cb) => setTimeoutShim(cb, 0);
global.Date = Object.assign(
  function () { return new (class { getTime() { return now; } })(); },
  { now: () => now }
);

function extractScript(html) {
  const m = html.match(/<script>([\s\S]*?)<\/script>/);
  if (!m) throw new Error('no <script> found in overlay HTML');
  return m[1];
}

const htmlPath = path.join(__dirname, '..', 'liveaudio', 'assets', 'subtitulos_obs.html');
const html = fs.readFileSync(htmlPath, 'utf8');
const script = extractScript(html);

// Run the overlay script. connect() runs at the end and creates the WS shim.
// eslint-disable-next-line no-eval
(0, eval)(script);

function liveCount() { return containerEl.querySelectorAll('.sub-box').length; }
function ribbonActive() { return containerEl.classList.contains('ribbon-active'); }
function liveTexts() {
  return containerEl.querySelectorAll('.sub-box').map((n) => n.textContent);
}

// ===========================================================================
// SCENARIO 1: promote -> drain -> demote -> spaced single line  (FIX 1)
// ===========================================================================
function scenario1() {
  // Fast burst of 3 lines within the debounce window -> promote to RIBBON.
  send({ text: 'a', style: 'default' });
  send({ text: 'b', style: 'default' });
  send({ text: 'c', style: 'default' });
  advance(500); // drain the pump (80ms debounce paces each)
  const promotedRibbon = ribbonActive();

  // Drain: age out all stacked boxes (5000 hide + 650 cleanup) so the ribbon
  // empties and the settle window demotes back to SINGLE.
  advance(7000);
  const demotedToSingle = !ribbonActive();
  const emptyAfterDrain = liveCount();

  // Spaced single line AFTER demotion: must render via the single path. If
  // isShowing were orphaned at true, this line would be SWALLOWED (FIX 1).
  send({ text: 'lonely', style: 'default' });
  advance(500);
  const singleRendered = liveCount();
  const singleText = liveTexts();

  return { promotedRibbon, demotedToSingle, emptyAfterDrain, singleRendered, singleText };
}

// ===========================================================================
// SCENARIO 2: cap overflow evicts the OLDEST box  (FIX 2)
// ===========================================================================
function scenario2() {
  // ribbonMaxLines default is 3. Send 5 distinct lines fast (paced > debounce).
  const texts = ['L0', 'L1', 'L2', 'L3', 'L4'];
  for (const t of texts) {
    send({ text: t, style: 'default' });
    advance(120); // > DEBOUNCE_MS so each renders immediately
  }
  advance(200);
  const lt = liveTexts();
  return {
    cap: 3,
    liveCount: lt.length,
    liveTexts: lt,
    // OLDEST (L0, L1) must be evicted; NEWEST (L4) must survive.
    oldestEvicted: !lt.includes('L0') && !lt.includes('L1'),
    newestKept: lt.includes('L4'),
  };
}

// ===========================================================================
// SCENARIO 3: replay burst then SILENCE demotes cleanly  (FIX 3)
// ===========================================================================
function scenario3() {
  // A catch-up replay burst sets replayActive = true on each enqueue.
  send({ text: 'r0', style: 'default', is_replay: true });
  send({ text: 'r1', style: 'default', is_replay: true });
  advance(300);
  const ribbonDuringReplay = ribbonActive();

  // Then SILENCE: no more traffic. Lines age out; the settle callback must age
  // out replayActive so demotion completes (FIX 3). Without the fix the overlay
  // is stuck in RIBBON forever.
  advance(7000);
  const demotedAfterSilence = !ribbonActive();

  return { ribbonDuringReplay, demotedAfterSilence };
}

// ===========================================================================
// SCENARIO 4: SPACED replay must NOT flap off mid-stream  (FIX 3b)
// ===========================================================================
// Catch-up replay payloads arrive spaced by catchup_interval_sec, which is
// LARGER than DEBOUNCE_MS (80ms). The settle timer fires in the gap between two
// replay payloads. With the unconditional latch clear (the regression) it sees
// pend<=1, live<=1, clears replayActive, the demote guard passes, and the overlay
// collapses RIBBON->SINGLE mid-replay; the next payload re-promotes => visible
// flap. The idle-gated clear must keep the ribbon stable until replay is silent
// past replayIdleMs.
function scenario4() {
  // Record EVERY ribbon-active transition so we can prove no toggle-off occurs
  // while replay is ongoing.
  const transitions = [];
  let prev = ribbonActive();
  transitions.push({ at: now, active: prev });
  function record() {
    const cur = ribbonActive();
    if (cur !== prev) {
      transitions.push({ at: now, active: cur });
      prev = cur;
    }
  }

  // 3 replay payloads spaced by 110ms (> DEBOUNCE_MS=80, < replayIdleMs=1500).
  // catchup_interval_sec omitted -> replayIdleMs falls back to the floor (1500ms).
  const GAP = 110;
  send({ text: 'r0', style: 'default', is_replay: true });
  record();
  advance(GAP);
  record();
  send({ text: 'r1', style: 'default', is_replay: true });
  record();
  advance(GAP);
  record();
  send({ text: 'r2', style: 'default', is_replay: true });
  record();
  advance(GAP);
  record();

  const ribbonDuringReplay = ribbonActive();
  // No transition may have turned the ribbon OFF at any point so far.
  const demotedMidStream = transitions.some((t, i) => i > 0 && t.active === false);

  // Now SILENCE. Lines expire (5000 hide + 650 cleanup) which fires
  // scheduleDemoteCheck. By then Date.now() - lastReplayAt >> replayIdleMs, so the
  // latch ages out and demotion completes — no wedge.
  advance(7000);
  record();
  const demotedAfterIdle = !ribbonActive();

  return {
    ribbonDuringReplay,
    demotedMidStream,
    demotedAfterIdle,
    transitions,
  };
}

// ===========================================================================
// SCENARIO 5: FLIP smoothing — survivors glide, the new line keeps its own enter
// ===========================================================================
// When the stacked ribbon reflows (oldest evicted + new line appended) the
// SURVIVING lines must be animated from their old position to the new one (FLIP,
// via element.animate) so they glide instead of jumping. The just-added line must
// NOT be FLIP-animated (it has its own enter animation). Pure visual smoothing:
// the cap and the per-line lifetime are unaffected.
function scenario5() {
  // Fast burst of 3 lines -> promote to RIBBON and fill it to the default cap (3).
  send({ text: 'L0', style: 'default' });
  send({ text: 'L1', style: 'default' });
  send({ text: 'L2', style: 'default' });
  advance(500); // pump all three stacked lines into the ribbon
  const promoted = ribbonActive();
  const beforeTexts = liveTexts();

  // Measure ONLY the next reflow: clear animations recorded during the fill.
  for (const el of containerEl.children) el._animations = [];

  // One more line forces evict(oldest=L0) + append(L3). Survivors L1,L2 must
  // glide; the NEW line L3 must NOT be FLIP-animated.
  send({ text: 'L3', style: 'default' });
  advance(500);
  const afterTexts = liveTexts();

  let survivorAnimations = 0;
  let newLineAnimations = 0;
  for (const el of containerEl.children) {
    const n = (el._animations || []).length;
    if (el.textContent === 'L3') newLineAnimations += n;
    else survivorAnimations += n;
  }

  return {
    promoted,
    beforeTexts,
    afterTexts,
    survivorsGlided: survivorAnimations > 0,
    newLineNotFlipped: newLineAnimations === 0,
    stillCapped: afterTexts.length === 3,
  };
}

function helloGate() {
  wsInstance.onopen();
  wsInstance.onmessage({ data: JSON.stringify({ text: 'foreign', style: 'default' }) });
  const beforeHello = liveCount();
  wsInstance.onmessage({ data: JSON.stringify({ type: 'hello', app: 'liveaudio', proto: 1, port: 8765 }) });
  wsInstance.onmessage({ data: JSON.stringify({ text: 'accepted', style: 'default' }) });
  advance(100);
  return { beforeHello, afterHello: liveTexts() };
}

function helloTimeout() {
  wsInstance.onopen();
  advance(2000);
  return { firstClosed: wsInstances[0].closed, urls: wsInstances.map((item) => item.url) };
}

function activeReconnect() {
  wsInstance.onopen();
  wsInstance.onmessage({ data: JSON.stringify({ type: 'hello', app: 'liveaudio', proto: 1, port: 8766 }) });
  wsInstance.close();
  advance(1000);
  return { urls: wsInstances.map((item) => item.url) };
}

function staleCallbacks() {
  const stale = wsInstance;
  stale.onopen();
  advance(2000);
  const before = wsInstances.length;
  stale.onmessage({ data: JSON.stringify({ type: 'hello', app: 'liveaudio', proto: 1, port: 8765 }) });
  stale.onmessage({ data: JSON.stringify({ text: 'stale', style: 'default' }) });
  stale.onclose();
  advance(0);
  return { before, after: wsInstances.length, rendered: liveTexts() };
}

function backoffCap() {
  const expected = [1000, 2000, 4000, 8000, 8000];
  const backoffs = [];
  for (const delay of expected) {
    for (let i = 0; i < 10; i++) {
      wsInstance.onopen();
      wsInstance.close();
      advance(0);
    }
    backoffs.push(scheduledDelays[scheduledDelays.length - 1]);
    advance(delay);
  }
  return { backoffs };
}

const which = process.argv[2];
const dispatch = { scenario1, scenario2, scenario3, scenario4, scenario5, helloGate, helloTimeout, activeReconnect, staleCallbacks, backoffCap };
if (!dispatch[which]) {
  process.stderr.write(`unknown scenario: ${which}\n`);
  process.exit(2);
}
process.stdout.write(JSON.stringify(dispatch[which]()));
