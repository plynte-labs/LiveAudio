// SPDX-License-Identifier: MIT
/**
 * Headless test harness executing <script> from assets/subtitulos_obs.html under Node.js.
 * Drives the adaptive SINGLE <-> RIBBON state machine and WebSocket protocol through
 * isolated scenarios using virtualized timers and a lightweight DOM shim.
 */

'use strict';

const fs = require('node:fs');
const path = require('node:path');
const childProcess = require('node:child_process');

function runScenario(name, customHtmlPath) {
  const env = { ...process.env };
  if (customHtmlPath) {
    env.OVERLAY_HTML_PATH = customHtmlPath;
  }
  const stdout = childProcess.execFileSync(process.execPath, [__filename, name], {
    encoding: 'utf8',
    env,
  });
  return JSON.parse(stdout.trim());
}

// When executed as a standalone process (via CLI or via runScenario),
// set up the headless shims, evaluate the overlay script, and dispatch the scenario.
if (require.main === module) {
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
      getBoundingClientRect() {
        const p = node.parentNode;
        const idx = p && p.children ? p.children.indexOf(node) : 0;
        return { top: idx * 50, bottom: idx * 50 + 40, left: 0, right: 0, width: 0, height: 40 };
      },
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

  // ---- Document visibility + SUSPENDABLE rAF --------------------------------
  const rafPending = [];
  const visibilityListeners = [];

  const documentShim = {
    hidden: false,
    getElementById() { return containerEl; },
    createElement,
    createTextNode(t) { return { tagName: null, _text: String(t), parentNode: null }; },
    get documentElement() { return documentElementEl; },
    addEventListener(type, cb) {
      if (type === 'visibilitychange' && typeof cb === 'function') visibilityListeners.push(cb);
    },
  };

  function setHidden(hidden) {
    documentShim.hidden = hidden;
    for (const cb of visibilityListeners.slice()) cb();
    if (!hidden) {
      const due = rafPending.splice(0, rafPending.length);
      for (const cb of due) cb();
    }
  }

  // ---- WebSocket shim --------------------------------------------------------
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
  global.window = { location: { search: process.argv[2] === 'partialRibbon' ? '?mode=ribbon' : '' } };
  global.URLSearchParams = URLSearchParams;
  global.WebSocket = WebSocketShim;
  global.console = { log() {}, warn() {}, error() {} };
  global.setTimeout = setTimeoutShim;
  global.clearTimeout = clearTimeoutShim;
  global.requestAnimationFrame = (cb) => {
    if (documentShim.hidden) {
      rafPending.push(cb);
      return 0;
    }
    return setTimeoutShim(cb, 0);
  };
  global.Date = Object.assign(
    function () { return new (class { getTime() { return now; } })(); },
    { now: () => now }
  );

  function extractScript(html) {
    const m = html.match(/<script>([\s\S]*?)<\/script>/);
    if (!m) throw new Error('no <script> found in overlay HTML');
    return m[1];
  }

  const htmlPath = process.env.OVERLAY_HTML_PATH || path.resolve(__dirname, '..', 'assets', 'subtitulos_obs.html');
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
    send({ text: 'a', style: 'default' });
    send({ text: 'b', style: 'default' });
    send({ text: 'c', style: 'default' });
    advance(500);
    const promotedRibbon = ribbonActive();

    advance(7000);
    const demotedToSingle = !ribbonActive();
    const emptyAfterDrain = liveCount();

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
    const texts = ['L0', 'L1', 'L2', 'L3', 'L4'];
    for (const t of texts) {
      send({ text: t, style: 'default' });
      advance(120);
    }
    advance(200);
    const lt = liveTexts();
    return {
      cap: 3,
      liveCount: lt.length,
      liveTexts: lt,
      oldestEvicted: !lt.includes('L0') && !lt.includes('L1'),
      newestKept: lt.includes('L4'),
    };
  }

  // ===========================================================================
  // SCENARIO 3: replay burst then SILENCE demotes cleanly  (FIX 3)
  // ===========================================================================
  function scenario3() {
    send({ text: 'r0', style: 'default', is_replay: true });
    send({ text: 'r1', style: 'default', is_replay: true });
    advance(300);
    const ribbonDuringReplay = ribbonActive();

    advance(7000);
    const demotedAfterSilence = !ribbonActive();

    return { ribbonDuringReplay, demotedAfterSilence };
  }

  // ===========================================================================
  // SCENARIO 4: SPACED replay must NOT flap off mid-stream  (FIX 3b)
  // ===========================================================================
  function scenario4() {
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
    const demotedMidStream = transitions.some((t, i) => i > 0 && t.active === false);

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
  function scenario5() {
    send({ text: 'L0', style: 'default' });
    send({ text: 'L1', style: 'default' });
    send({ text: 'L2', style: 'default' });
    advance(500);
    const promoted = ribbonActive();
    const beforeTexts = liveTexts();

    for (const el of containerEl.children) el._animations = [];

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

  // ===========================================================================
  // SCENARIO revealWithoutRaf: the reveal must not depend on requestAnimationFrame
  // ===========================================================================
  function revealWithoutRaf() {
    setHidden(true);
    const styles = ['default', 'neon', 'minimal', 'bold'];
    const revealed = {};
    for (const style of styles) {
      const text = `t-${style}`;
      send({ text, style });
      advance(100);
      const box = containerEl.querySelectorAll('.sub-box').find((b) => b.textContent === text);
      revealed[style] = !!box && box.classList.contains('visible');
      advance(6000);
    }
    return { revealed, rafPending: rafPending.length };
  }

  // ===========================================================================
  // SCENARIO visibilityReset: becoming visible HARD RESETS the overlay
  // ===========================================================================
  function visibilityReset() {
    setHidden(true);
    for (const text of ['h0', 'h1', 'h2']) {
      send({ text, style: 'default' });
      advance(120);
    }
    advance(6000);

    send({ text: 'stale', style: 'default' });
    advance(300);
    const liveWhileHidden = liveTexts();
    const staleBox = containerEl.querySelectorAll('.sub-box')[0] || null;

    setHidden(false);
    advance(0);
    const afterUnhide = liveTexts();

    send({ text: 'fresh', style: 'default' });
    advance(300);
    const fresh = containerEl.querySelectorAll('.sub-box');

    return {
      liveWhileHidden,
      afterUnhide,
      ribbonAfterUnhide: ribbonActive(),
      staleDroppedSilently: !!staleBox && staleBox.parentNode === null
        && !staleBox.classList.contains('hiding'),
      freshTexts: fresh.map((n) => n.textContent),
      freshVisible: fresh.length === 1 && fresh[0].classList.contains('visible'),
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

  // ===========================================================================
  // SCENARIO: provisional (partial) subtitles share the final's slot
  // ===========================================================================
  function partialSubtitle() {
    const texts = () => liveTexts();
    const boxes = () => containerEl.querySelectorAll('.sub-box');
    send({ type: 'subtitle_partial', utterance_id: 'utt_1', partial_text: 'hel' });
    const first = texts();
    const partialNode = boxes()[0];
    send({ type: 'subtitle_partial', utterance_id: 'utt_1', partial_text: 'hello wor' });
    const overwritten = texts();
    const reusedNode = boxes()[0] === partialNode;
    advance(200);
    const afterIdle = texts();

    send({ id: 'utt_1', text: 'hello world', style: 'default' });
    const finalVisibleAtOnce = boxes().length === 1 && boxes()[0].classList.contains('visible');
    const finalReplacedNode = boxes()[0] !== partialNode;
    advance(200);
    const finals = texts();

    // A new partial takes over the slot of the previous (still visible) final.
    send({ type: 'subtitle_partial', utterance_id: 'utt_2', partial_text: 'next' });
    const takeover = texts();
    send({ id: 'utt_2', text: 'next one', style: 'default' });
    advance(200);
    const clearedByOwnFinal = texts();

    send({ type: 'subtitle_partial', utterance_id: 'utt_3', partial_text: 'abc' });
    send({ type: 'subtitle_partial', utterance_id: 'utt_3', partial_text: '' });
    const clearedByEmpty = texts();

    send({ type: 'subtitle_partial', utterance_id: 'utt_4', partial_text: 'abc' });
    setHidden(true);
    setHidden(false);
    const clearedOnResume = texts();

    return { first, overwritten, reusedNode, afterIdle, finalVisibleAtOnce, finalReplacedNode, finals, takeover, clearedByOwnFinal, clearedByEmpty, clearedOnResume };
  }

  function partialRibbon() {
    const texts = () => liveTexts();
    send({ id: 'utt_1', text: 'A', style: 'default' });
    advance(200);
    send({ id: 'utt_2', text: 'B', style: 'default' });
    advance(200);
    send({ type: 'subtitle_partial', utterance_id: 'utt_3', partial_text: 'par' });
    const withPartial = texts();
    send({ type: 'subtitle_partial', utterance_id: 'utt_3', partial_text: 'partial' });
    const updated = texts();
    send({ id: 'utt_3', text: 'final', style: 'default' });
    advance(200);
    const afterFinal = texts();
    send({ type: 'subtitle_partial', utterance_id: 'utt_4', partial_text: 'x' });
    send({ type: 'subtitle_partial', utterance_id: 'utt_4', partial_text: '' });
    const clearedByEmpty = texts();
    return { withPartial, updated, afterFinal, clearedByEmpty, ribbon: ribbonActive() };
  }

  const which = process.argv[2];
  const dispatch = {
    partialSubtitle,
    partialRibbon,
    scenario1,
    scenario2,
    scenario3,
    scenario4,
    scenario5,
    revealWithoutRaf,
    visibilityReset,
    helloGate,
    helloTimeout,
    activeReconnect,
    staleCallbacks,
    backoffCap,
  };

  if (!dispatch[which]) {
    process.stderr.write(`unknown scenario: ${which}\n`);
    process.exit(2);
  }

  process.stdout.write(JSON.stringify(dispatch[which]()));
}

module.exports = {
  runScenario,
  dispatch: {
    partialSubtitle: (hp) => runScenario('partialSubtitle', hp),
    partialRibbon: (hp) => runScenario('partialRibbon', hp),
    scenario1: (hp) => runScenario('scenario1', hp),
    scenario2: (hp) => runScenario('scenario2', hp),
    scenario3: (hp) => runScenario('scenario3', hp),
    scenario4: (hp) => runScenario('scenario4', hp),
    scenario5: (hp) => runScenario('scenario5', hp),
    revealWithoutRaf: (hp) => runScenario('revealWithoutRaf', hp),
    visibilityReset: (hp) => runScenario('visibilityReset', hp),
    helloGate: (hp) => runScenario('helloGate', hp),
    helloTimeout: (hp) => runScenario('helloTimeout', hp),
    activeReconnect: (hp) => runScenario('activeReconnect', hp),
    staleCallbacks: (hp) => runScenario('staleCallbacks', hp),
    backoffCap: (hp) => runScenario('backoffCap', hp),
  },
  scenario1: (hp) => runScenario('scenario1', hp),
  scenario2: (hp) => runScenario('scenario2', hp),
  scenario3: (hp) => runScenario('scenario3', hp),
  scenario4: (hp) => runScenario('scenario4', hp),
  scenario5: (hp) => runScenario('scenario5', hp),
  revealWithoutRaf: (hp) => runScenario('revealWithoutRaf', hp),
  visibilityReset: (hp) => runScenario('visibilityReset', hp),
  helloGate: (hp) => runScenario('helloGate', hp),
  helloTimeout: (hp) => runScenario('helloTimeout', hp),
  activeReconnect: (hp) => runScenario('activeReconnect', hp),
  staleCallbacks: (hp) => runScenario('staleCallbacks', hp),
  backoffCap: (hp) => runScenario('backoffCap', hp),
};
