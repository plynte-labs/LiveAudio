// SPDX-License-Identifier: MIT

/**
 * Frontend automated verification test suite for LiveAudio Desktop (Tauri 2).
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { bindProfileSelection, bindTabNavigation, externalConfigUpdate, persistAndApplyRuntimeConfig, runtimeSettingsChanged } from './src/runtime-settings.js';
import * as runtimeSettings from './src/runtime-settings.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function parseHtmlTree(html) {
  const root = { tagName: '#document', classes: [], children: [], parent: null };
  const stack = [root];
  const voidTags = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'param', 'source', 'track', 'wbr']);
  const tokenPattern = /<!--[\s\S]*?-->|<![^>]*>|<\/?[a-z][\w:-]*(?:\s[^<>]*?)?\s*\/?>/gi;

  for (const [token] of html.matchAll(tokenPattern)) {
    if (token.startsWith('<!--') || token.startsWith('<!')) continue;
    const closing = /^<\//.test(token);
    const tagName = token.match(/^<\/?\s*([a-z][\w:-]*)/i)?.[1]?.toLowerCase();
    if (!tagName) continue;

    if (closing) {
      for (let index = stack.length - 1; index > 0; index--) {
        if (stack[index].tagName === tagName) {
          stack.length = index;
          break;
        }
      }
      continue;
    }

    const classes = token.match(/\bclass\s*=\s*["']([^"']*)["']/i)?.[1]?.split(/\s+/) || [];
    const node = { tagName, classes, children: [], parent: stack.at(-1) };
    node.parent.children.push(node);
    if (!voidTags.has(tagName) && !/\/\s*>$/.test(token)) stack.push(node);
  }

  return root;
}

function findElement(node, predicate) {
  if (predicate(node)) return node;
  for (const child of node.children) {
    const found = findElement(child, predicate);
    if (found) return found;
  }
  return null;
}

// Loads the real src/app.js in a vm-like scope (no DOM, no Tauri). Its three imports are bound to the
// real modules; the DOM, IPC, timers and storage are stubs. Timers are queued and run on demand, and a
// fake backend mirrors the Rust commands the Style tab uses (style saves keep the other fields).
async function loadAppHarness(initialConfig, options = {}) {
  const appSource = fs.readFileSync(path.join(__dirname, 'src', 'app.js'), 'utf8');
  const modules = {
    './runtime-settings.js': await import('./src/runtime-settings.js'),
    './i18n.js': await import('./src/i18n.js'),
    './subtitle-style.js': await import('./src/subtitle-style.js'),
  };
  if (typeof globalThis.localStorage === 'undefined') {
    const memory = new Map();
    globalThis.localStorage = { getItem: (key) => (memory.has(key) ? memory.get(key) : null), setItem: (key, value) => memory.set(key, String(value)), removeItem: (key) => memory.delete(key) };
  }
  const timers = [];
  let timerSeq = 0;
  const cache = new Map();
  const handlers = new Map();
  const listeners = new Map();
  const backend = {
    config: structuredClone(initialConfig),
    calls: [],
    holdStyleSaves: false,
    failStyleSaves: 0,
    gate: null,
    release() { const resolve = backend.resolveGate; backend.resolveGate = null; backend.gate = null; resolve?.(); },
  };
  Object.assign(backend, options);
  const defaults = { ...structuredClone(initialConfig), subtitle_style: 'default', subtitle_theme: {}, subtitle_presets: [] };

  const makeElement = (id) => {
    const element = {
      id, value: '', checked: false, disabled: false, hidden: false, textContent: '', innerHTML: '', title: '',
      className: '', tabIndex: 0, options: [], children: [], dataset: {}, min: '', max: '', step: '', files: [],
      style: { setProperty() {}, removeProperty() {} },
      classList: { add() {}, remove() {}, toggle() {}, contains() { return false; } },
      addEventListener(type, fn) { const byType = listeners.get(element) ?? {}; (byType[type] ??= []).push(fn); listeners.set(element, byType); },
      dispatch(type, event = {}) { for (const fn of listeners.get(element)?.[type] ?? []) fn({ target: element, ...event }); },
      setAttribute() {}, removeAttribute() {}, getAttribute() { return null; }, focus() {}, blur() {},
      click() { element.dispatch('click'); },
      replaceChildren() {}, appendChild() {}, remove() {}, querySelector() { return null; }, querySelectorAll() { return []; },
    };
    return new Proxy(element, {
      get(target, prop) {
        if (prop in target || typeof prop === 'symbol' || prop === 'then') return target[prop];
        return () => undefined;
      },
      set(target, prop, value) { target[prop] = value; return true; },
    });
  };
  const elementFor = (id) => {
    if (!cache.has(id)) cache.set(id, makeElement(id));
    return cache.get(id);
  };
  const document = {
    readyState: 'complete',
    getElementById: elementFor,
    querySelector() { return makeElement('query'); },
    querySelectorAll() { return []; },
    createElement() { return makeElement('created'); },
    createTextNode(text) { return { textContent: text }; },
    addEventListener() {}, removeEventListener() {},
    documentElement: makeElement('html'), body: makeElement('body'), activeElement: null,
  };

  const statusFor = () => ({
    is_running: Boolean(backend.serviceRunning), is_pending: false, model_size: 'small', device: 'cpu', active_device: '', monitor_mode: false,
    active_backend: backend.activeBackend ?? null, backend_reason: backend.backendReason ?? null,
    audio_state: '', asr_state: '', ws_port: 8765, session_error: '', foreign_service_pid: null,
  });
  // Backend: a style save stores its fields at call time; its reply can be held, like a slow disk write.
  const invoke = async (cmd, args = {}) => {
    backend.calls.push({ cmd, args: structuredClone(args) });
    switch (cmd) {
      case 'get_config': return structuredClone(backend.config);
      case 'save_subtitle_style': {
        backend.config = {
          ...backend.config,
          subtitle_theme: structuredClone(args.theme),
          subtitle_presets: structuredClone(args.presets),
          subtitle_style: args.subtitleStyle ?? backend.config.subtitle_style,
        };
        const reply = structuredClone(backend.config);
        if (backend.failStyleSaves > 0) { backend.failStyleSaves -= 1; throw new Error('disk full'); }
        if (backend.holdStyleSaves) await (backend.gate ??= new Promise((resolve) => { backend.resolveGate = resolve; }));
        return reply;
      }
      case 'save_config': {
        backend.config = { ...structuredClone(args.config), subtitle_style: backend.config.subtitle_style, subtitle_theme: backend.config.subtitle_theme, subtitle_presets: backend.config.subtitle_presets };
        return structuredClone(backend.config);
      }
      case 'reset_config_defaults': backend.config = structuredClone(defaults); return structuredClone(defaults);
      case 'get_service_status': return statusFor();
      case 'restart_service':
        // A restart picks up the engine the config selects, as the real start does.
        if (backend.deriveSelected) backend.activeBackend = backend.config.device === 'cpu' || backend.config.asr_backend === 'ort' ? 'ort' : 'ct2_cuda';
        return statusFor();
      case 'get_gpu_backend_status': {
        const gpu = structuredClone(backend.gpuStatus ?? null);
        // Mirrors the Rust command: device cpu or asr_backend ort selects ORT, otherwise the installed runtime wins.
        if (gpu?.nvidia_runtime?.installed && backend.deriveSelected) {
          gpu.selected_backend = backend.config.device === 'cpu' || backend.config.asr_backend === 'ort' ? 'ort' : 'ct2_cuda';
        }
        return gpu;
      }
      case 'install_nvidia_runtime':
        if (backend.installError) throw new Error(backend.installError);
        if (backend.gpuStatus) backend.gpuStatus = { ...backend.gpuStatus, nvidia_runtime: { installed: true, missing: [] } };
        return null;
      case 'get_obs_overlay_info': return { url: '', connected: false };
      case 'get_usage_stats': return structuredClone(backend.usageStats ?? { audio_seconds: 0, subtitles: 0, sessions: 0, first_used: 0 });
      case 'get_support_environment': return 'LiveAudio 1.6.0 / OS: windows x86_64';
      case 'get_audio_devices': return [];
      case 'list_system_fonts': return backend.systemFonts ?? null;
      case 'get_profile_presets': return [];
      case 'get_recent_logs': return [];
      default: return null;
    }
  };
  const listen = async (name, callback) => { (handlers.get(name) ?? handlers.set(name, []).get(name)).push(callback); };
  const window = { __TAURI__: { core: { invoke }, event: { listen } }, matchMedia: () => ({ matches: Boolean(backend.reducedMotion) }), addEventListener() {}, innerWidth: 1200, innerHeight: 800 };
  const setTimer = (fn, ms = 0) => { const id = ++timerSeq; timers.push({ id, fn, ms }); return id; };
  const clearTimer = (id) => { const index = timers.findIndex((timer) => timer.id === id); if (index >= 0) timers.splice(index, 1); };
  const settle = async () => { for (let round = 0; round < 30; round++) await new Promise((resolve) => setImmediate(resolve)); };
  const runTimers = async () => { while (timers.length) { timers.shift().fn(); await settle(); } };
  class Option { constructor(text, value) { this.text = text; this.value = value; } }

  // The module's own `import` lines become bindings to the real modules; the hooks at the end expose state.
  const body = appSource.replace(/^import \{([^}]*)\} from '([^']+)';$/gm,
    (_, names, specifier) => `const {${names}} = __modules[${JSON.stringify(specifier)}];`);
  const hooks = `\n;return {
    get currentConfig() { return currentConfig; },
    get savedConfig() { return savedConfig; },
    get showConfirmModal() { return showConfirmModal; },
    get styleDirty() { return styleDirty; },
    styleChangesUnsaved() { return styleChangesUnsaved(); },
    confirmLeaveStyleTab() { return confirmLeaveStyleTab(); },
    advancedChangesUnsaved() { return advancedChangesUnsaved(); },
    confirmLeaveAdvancedTab() { return confirmLeaveAdvancedTab(); },
    setActiveTab(id) { return setActiveTab(id); },
    isFormDirty() { return JSON.stringify(collectFormData()) !== JSON.stringify(savedConfig); },
    formDiffKeys() { const now = collectFormData(); return Object.keys({ ...now, ...savedConfig }).filter((key) => JSON.stringify(now[key]) !== JSON.stringify(savedConfig[key])).map((key) => key + "=" + JSON.stringify(now[key]) + " vs " + JSON.stringify(savedConfig[key])); },
  };`;
  const factory = new Function('__modules', 'window', 'document', 'navigator', 'setTimeout', 'clearTimeout',
    'setInterval', 'Option', 'alert', 'confirm', body + hooks);
  const app = factory(modules, window, document, { userAgent: 'Win32', language: 'es-ES' }, setTimer, clearTimer,
    () => 0, Option, () => {}, () => true);
  await settle();
  return {
    app,
    backend,
    el: elementFor,
    timers,
    settle,
    runTimers,
    async emit(name, payload) { for (const callback of handlers.get(name) ?? []) callback({ payload }); await settle(); },
    styleSaves: () => backend.calls.filter((call) => call.cmd === 'save_subtitle_style'),
    async confirmModal() { elementFor('modal-btn-confirm').click(); await settle(); },
    cancelModal() { elementFor('modal-btn-cancel').click(); },
  };
}

async function runTests() {
  console.log('--- Running LiveAudio Desktop Frontend Verification Tests ---');
  let passed = 0;
  let failed = 0;

  function assert(condition, message) {
    if (condition) {
      console.log(`[PASS] ${message}`);
      passed++;
    } else {
      console.error(`[FAIL] ${message}`);
      failed++;
    }
  }

  // 1. Verify index.html exists and has required elements
  const htmlPath = path.join(__dirname, 'src', 'index.html');
  assert(fs.existsSync(htmlPath), 'index.html exists');
  const htmlContent = fs.readFileSync(htmlPath, 'utf8');

  // Verify status pills
  assert(htmlContent.includes('id="pill-status"'), 'Status pill element exists');
  assert(htmlContent.includes('id="pill-vad"'), 'VAD pill element exists');
  assert(htmlContent.includes('id="pill-ws"'), 'WebSocket port pill element exists');
  assert(htmlContent.includes('id="pill-hw"'), 'Hardware / model pill element exists');

  // Verify controls
  assert(htmlContent.includes('id="btn-toggle-service"'), 'Master Start/Stop toggle button exists');
  assert(htmlContent.includes('id="btn-save-config"'), 'Save config button exists');
  assert(htmlContent.includes('id="btn-reset-config"'), 'Reset defaults button exists');
  assert(htmlContent.includes('id="btn-export-diag"'), 'Export diagnostics button exists');

  // Verify tabs
  assert(htmlContent.includes('data-tab="tab-setup"'), 'Setup tab button exists');
  assert(htmlContent.includes('data-tab="tab-files"'), 'Files tab button exists');
  assert(htmlContent.includes('data-tab="tab-subtitles"'), 'Subtitles tab button exists');
  assert(htmlContent.includes('data-tab="tab-advanced"'), 'Advanced tab button exists');
  assert(htmlContent.includes('role="tablist"') && htmlContent.includes('role="tabpanel"'),
    'Settings navigation exposes tab semantics');
  assert(htmlContent.includes('data-i18n="tab_setup"')
    && htmlContent.includes('data-i18n="tab_files"'), 'Setup and Files tabs have localization keys');
  assert(htmlContent.includes('data-i18n-aria-label="settings_tabs_label"'),
    'Settings tablist has a localized accessible name');

  // Verify inputs
  assert(htmlContent.includes('id="select-audio-device"'), 'Audio device dropdown exists');
  assert(htmlContent.includes('id="select-compute-device"'), 'Compute device unified select dropdown exists');
  assert(htmlContent.includes('id="device-cuda"'), 'CUDA compute option exists');
  assert(htmlContent.includes('id="device-cpu"'), 'CPU compute option exists');
  assert(htmlContent.includes('id="select-model-size"'), 'Model size selector exists');
  assert(htmlContent.includes('id="select-subtitle-style"'), 'Subtitle theme selector exists');
  assert(htmlContent.includes('id="input-obs-url"'), 'OBS Browser Source URL readonly input exists');
  assert(htmlContent.includes('id="btn-copy-obs-url"'), 'Copy OBS URL button exists');
  assert(htmlContent.includes('id="input-blacklist"'), 'Unified blacklist input exists');
  assert(htmlContent.includes('id="badge-blacklist-lang"'), 'Blacklist active language indicator badge exists');
  assert(htmlContent.includes('id="prompt-es"'), 'Unified prompt input exists');
  assert(htmlContent.includes('id="badge-prompt-lang"'), 'Prompt active language indicator badge exists');
  assert(htmlContent.includes('id="btn-open-session-folder"') && htmlContent.includes('id="session-path"'), 'Subtitles & session folder button and path exist in UI');
  assert(htmlContent.includes('id="slider-max-chunk"') && htmlContent.includes('max="30.0"'), 'Max chunk duration slider caps at the 30.0s worker window');
  assert(htmlContent.includes('v1.6.0'), 'Version 1.6.0 badge rendered in index.html');
  const authorLink = htmlContent.match(/<a\b[^>]*id="author-profile-link"[^>]*>([\s\S]*?)<\/a>/i);
  assert(authorLink && authorLink[0].includes('data-i18n-aria-label="author_credit_accessible"') && authorLink[1] === 'FranGuh',
    'Author credit is a localized-aria anchor in the About footer');
  assert(authorLink && /href="https:\/\/github\.com\/franguh"/.test(authorLink[0])
    && /target="_blank"/.test(authorLink[0])
    && /rel="noopener noreferrer"/.test(authorLink[0]),
  'Author link targets the fixed external profile safely');
  assert(authorLink && /Plynte Labs \/ <a\b[^>]*id="author-profile-link"/.test(htmlContent),
    'Author credit follows Plynte Labs in the About footer');

  // 2. Verify styles.css exists and covers all subtitulos_obs.html styles
  const cssPath = path.join(__dirname, 'src', 'styles.css');
  assert(fs.existsSync(cssPath), 'styles.css exists');
  const cssContent = fs.readFileSync(cssPath, 'utf8');
  assert(/\.author-profile-link\s*\{[^}]*app-region:\s*no-drag/s.test(cssContent),
    'Author link opts out of the draggable titlebar region');
  assert(/\.author-profile-link:focus-visible[\s\S]*?\{[^}]*outline/s.test(cssContent),
    'Author link has a visible keyboard focus indicator');

  const requiredStyles = [
    'style-default',
    'style-karaoke',
    'style-neon',
    'style-minimal',
    'style-bold',
    'style-rgb',
    'style-typewriter',
  ];

  // The preview styles come from the overlay itself (generated overlay-preview.css), not from styles.css
  const previewCss = fs.readFileSync(path.join(__dirname, 'src', 'overlay-preview.css'), 'utf8');
  requiredStyles.forEach((style) => {
    assert(previewCss.includes(`.${style}`), `Preview CSS includes class .${style}`);
  });

  // Verify key animations
  assert(previewCss.includes('@keyframes popIn'), 'Karaoke popIn animation exists');
  assert(previewCss.includes('@keyframes rgbFadeIn'), 'RGB rgbFadeIn animation exists');
  assert(previewCss.includes('@keyframes typeIn'), 'Typewriter typeIn animation exists');
  assert(cssContent.includes('@keyframes pulse'), 'VAD glowing pulse animation exists');

  // 3. Verify app.js logic
  const jsPath = path.join(__dirname, 'src', 'app.js');
  assert(fs.existsSync(jsPath), 'app.js exists');
  const jsContent = fs.readFileSync(jsPath, 'utf8');

  assert(jsContent.includes('window.__TAURI__.core.invoke'), 'Tauri core invoke call handled');
  assert(jsContent.includes('window.__TAURI__.event.listen'), 'Tauri event listener handled');
  // Tauri 2 maps command arguments from camelCase in JS to snake_case in Rust;
  // a snake_case key is silently ignored by the backend.
  const snakeInvokeKeys = [...jsContent.matchAll(/invoke\('\w+',\s*\{([^}]*)\}/g)]
    .flatMap(([, body]) => body.split(',').map((entry) => entry.split(':')[0].trim()))
    .filter((key) => key.includes('_'));
  assert(snakeInvokeKeys.length === 0,
    `invoke arguments use camelCase keys (found: ${snakeInvokeKeys.join(', ') || 'none'})`);
  assert(jsContent.includes("invoke('get_recent_logs', { maxLines: 40 })"),
    'get_recent_logs passes maxLines in camelCase');
  assert(jsContent.includes('get_config'), 'get_config command invoked');
  assert(jsContent.includes('save_config'), 'save_config command invoked');
  assert(jsContent.includes('get_audio_devices'), 'get_audio_devices command invoked');
  assert(jsContent.includes('get_profile_presets'), 'profile presets loaded from Rust');
  assert(jsContent.includes('open_session_folder'), 'session folder action uses backend IPC');
  assert(/elements\.authorProfileLink\.addEventListener\('click',[\s\S]*?if\s*\(!isTauri\)[\s\S]*?event\.preventDefault\(\)[\s\S]*?invoke\('open_author_profile'\)/.test(jsContent),
    'Tauri author-link clicks use the fixed native browser command');
  assert(!/invoke\('open_author_profile',\s*\{/.test(jsContent),
    'Author profile URL is not supplied by frontend input');
  assert(jsContent.includes("t('toast_author_profile_open_failed')"),
    'External-link failures use localized feedback');
  const nativeCommands = fs.readFileSync(path.join(__dirname, 'src-tauri', 'src', 'commands.rs'), 'utf8');
  const nativeLib = fs.readFileSync(path.join(__dirname, 'src-tauri', 'src', 'lib.rs'), 'utf8');
  assert(/pub async fn open_author_profile\(\)/.test(nativeCommands)
    && nativeCommands.includes('https://github.com/franguh'),
  'Native author opener uses a fixed profile URL and accepts no frontend argument');
  assert(nativeLib.includes('commands::open_author_profile'),
    'Native author opener is registered with Tauri');
  const i18nContent = fs.readFileSync(path.join(__dirname, 'src', 'i18n.js'), 'utf8');
  assert(!htmlContent.includes('by Franguh') && !i18nContent.includes('author_credit:'),
    'Header no longer carries the by-Franguh credit');
  assert(i18nContent.includes('author_credit_accessible:')
    && i18nContent.includes('toast_author_profile_open_failed:'),
  'Author link has localized accessible context and failure feedback');
  assert(jsContent.includes('get_service_status'), 'get_service_status command invoked');
  assert(jsContent.includes('start_service'), 'start_service command invoked');
  assert(jsContent.includes('stop_service'), 'stop_service command invoked');
  assert(jsContent.includes('get_obs_overlay_info'), 'get_obs_overlay_info command invoked');
  assert(jsContent.includes('export_diagnostics'), 'export_diagnostics command invoked');
  assert(jsContent.includes('send_test_subtitle'), 'send_test_subtitle command invoked');
  assert(jsContent.includes('persistAndApplyRuntimeConfig') && jsContent.includes('window.confirm'),
    'saving active runtime changes asks for restart confirmation');
  assert(jsContent.includes('checkRnnoiseEnabled.checked = Boolean(config.rnnoise_enabled)')
    && jsContent.includes('checkAcousticEventsEnabled.checked = Boolean(config.acoustic_events_enabled)')
    && jsContent.includes('cfg.rnnoise_enabled = elements.checkRnnoiseEnabled.checked')
    && jsContent.includes('cfg.acoustic_events_enabled = elements.checkAcousticEventsEnabled.checked'),
    'optional audio settings round-trip through the desktop draft and saved config');
  assert(jsContent.includes('checkLivePartialSubtitles.checked = Boolean(config.live_partial_subtitles_enabled)')
    && jsContent.includes('cfg.live_partial_subtitles_enabled = elements.checkLivePartialSubtitles.checked'),
    'the live partial subtitles flag round-trips through the desktop draft and saved config');
  assert(htmlContent.includes('id="check-live-partial-subtitles-enabled"')
    && htmlContent.includes('data-i18n="lbl_live_partial_subtitles"')
    && htmlContent.includes('data-i18n="hint_live_partial_subtitles"'),
    'the live partial subtitles switch has a control, label and hint');
  assert(jsContent.includes("listen('subtitle_partial'")
    && jsContent.includes("listen('subtitle_cue'")
    && htmlContent.includes('id="feed-partial-line"'),
    'the preview listens for provisional text and has a dedicated line outside the feed');
  assert(htmlContent.includes('id="select-profile"')
    && htmlContent.includes('id="slider-catchup-interval"')
    && htmlContent.includes('id="btn-open-session-folder"')
    && htmlContent.includes('id="check-continuous"')
    && htmlContent.includes('id="check-save-vtt"'),
    'profile, subtitle timing and session file controls are present');
  assert(htmlContent.includes('id="check-rnnoise-enabled"')
    && htmlContent.includes('id="check-acoustic-events-enabled"'),
    'optional audio settings expose both accessible controls');
  assert(htmlContent.includes('data-i18n="hint_rnnoise_enabled"')
    && htmlContent.includes('data-i18n="hint_acoustic_events_enabled"'),
    'optional audio settings explain their current availability');
  assert(/<label class="switch-label">\s*<span class="switch-track"><input type="checkbox" id="check-rnnoise-enabled"/.test(htmlContent)
    && /<label class="switch-label">\s*<span class="switch-track"><input type="checkbox" id="check-acoustic-events-enabled"/.test(htmlContent),
    'optional audio switches have accessible labels');
  assert(htmlContent.includes('aria-describedby="hint-rnnoise-enabled"')
    && htmlContent.includes('aria-describedby="hint-acoustic-events-enabled"'),
    'optional audio switches expose their help text to assistive technology');

  const setupPane = htmlContent.match(/<section id="tab-setup"[\s\S]*?<\/section>/)?.[0] || '';
  const subtitlePane = htmlContent.match(/<section id="tab-subtitles"[\s\S]*?<\/section>/)?.[0] || '';
  const advancedPane = htmlContent.match(/<section id="tab-advanced"[\s\S]*?<\/section>/)?.[0] || '';
  const filesPane = htmlContent.match(/<section id="tab-files"[\s\S]*?<\/section>/)?.[0] || '';
  const allIds = [...htmlContent.matchAll(/\bid="([^"]+)"/g)].map((match) => match[1]);
  assert(new Set(allIds).size === allIds.length, 'Reorganized settings retain unique control IDs');
  const layoutTree = parseHtmlTree(htmlContent);
  const mainElement = findElement(layoutTree, (node) => node.tagName === 'main');
  const leftPane = findElement(layoutTree, (node) => node.classes.includes('left-pane'));
  const rightPane = findElement(layoutTree, (node) => node.classes.includes('right-pane'));
  assert(Boolean(mainElement && leftPane && rightPane
    && leftPane.parent === mainElement && rightPane.parent === mainElement),
  'Left settings and right preview panes are direct sibling children of main');
  const appSource = fs.readFileSync(path.join(__dirname, 'src', 'app.js'), 'utf8');
  const appLookupIds = [...appSource.matchAll(/document\.getElementById\(['"]([^'"]+)['"]\)/g)]
    .map((match) => match[1]);
  const missingOrDuplicatedLookups = appLookupIds.filter((id) => allIds.filter((htmlId) => htmlId === id).length !== 1);
  assert(missingOrDuplicatedLookups.length === 0,
    `Every literal app.js element lookup maps to one HTML ID${missingOrDuplicatedLookups.length ? `: ${missingOrDuplicatedLookups.join(', ')}` : ''}`);
  assert(setupPane.includes('select-audio-device') && setupPane.includes('select-asr-lang')
    && setupPane.includes('select-profile'), 'Setup groups capture source, spoken language and profile');
  assert(subtitlePane.includes('select-display-mode') && !subtitlePane.includes('select-subtitle-style')
    && !subtitlePane.includes('select-backlog-policy'),
    'Subtitles focuses on presentation settings');
  assert(subtitlePane.includes('slider-ribbon-lines') && subtitlePane.includes('val-ribbon-lines'),
    'Subtitles retains the configured maximum Ribbon line control');
  assert(advancedPane.includes('select-compute-device') && advancedPane.includes('select-model-size')
    && advancedPane.includes('slider-vad-threshold') && advancedPane.includes('select-backlog-policy')
    && advancedPane.includes('group-prompt') && advancedPane.includes('group-blacklist'),
    'Advanced contains model, compute, VAD, backlog and language filters');
  assert(advancedPane.includes('card-adv-allowed-origins')
    && advancedPane.includes('allowed-ports-list')
    && advancedPane.includes('input-add-allowed-port')
    && advancedPane.includes('btn-add-allowed-port')
    && advancedPane.includes('error-add-allowed-port')
    && advancedPane.includes('recent-blocked-list'),
    'Advanced contains the allowed local connections card, port inputs, and blocked attempts list');
  assert(/<span id="error-add-allowed-port"[^>]*role="alert"[^>]*aria-live="polite"/.test(advancedPane),
    'Allowed port error element has accessible role and aria-live semantics');
  assert(appSource.includes("invoke('get_allowed_connections')")
    && appSource.includes("invoke('allow_loopback_port'")
    && appSource.includes("invoke('revoke_loopback_port'"),
    'app.js invokes get_allowed_connections, allow_loopback_port, and revoke_loopback_port');
  assert(filesPane.includes('btn-open-session-folder') && filesPane.includes('check-continuous')
    && filesPane.includes('check-save-transcript') && filesPane.includes('check-save-vtt'),
    'Files groups session folder, continuity and export settings');
  assert(htmlContent.includes('id="pill-status"') && htmlContent.includes('class="status-details"'),
    'Header keeps one primary status and tucks diagnostics into details');
  assert(cssContent.includes(':focus-visible') && cssContent.includes('prefers-reduced-motion: reduce'),
    'Styles provide visible keyboard focus and honor reduced-motion preference');
  assert(cssContent.includes('.obs-help summary:focus-visible'),
    'Collapsed OBS instructions expose a visible keyboard focus indicator');
  assert(appSource.includes('cfg.subtitle_ribbon_max_lines = parseInt(elements.sliderRibbonLines.value, 10)')
    && appSource.includes('elements.sliderRibbonLines.addEventListener'),
    'Ribbon line input remains wired to saved config and its value display');

  const tabPanes = ['setup', 'subtitles', 'advanced', 'files'].map((name) => ({
    id: `tab-${name}`,
    hidden: true,
    classList: {
      toggle(value, force) {
        if (force) this.active = value;
        else this.active = false;
        return force;
      },
    },
  }));
  const tabButtons = tabPanes.map((pane) => ({
    attributes: { 'aria-controls': pane.id },
    handlers: {},
    tabIndex: -1,
    classList: {
      values: new Set(),
      add(value) { this.values.add(value); },
      remove(value) { this.values.delete(value); },
      contains(value) { return this.values.has(value); },
      toggle(value, force) {
        if (force) this.values.add(value);
        else this.values.delete(value);
        return force;
      },
    },
    getAttribute(name) { return this.attributes[name]; },
    setAttribute(name, value) { this.attributes[name] = value; },
    addEventListener(name, callback) { this.handlers[name] = callback; },
    focus() { this.focused = true; },
  }));
  const tablist = {
    ownerDocument: { getElementById(id) { return tabPanes.find((pane) => pane.id === id); } },
    querySelectorAll(selector) { return selector === '[role="tab"]' ? tabButtons : []; },
  };
  bindTabNavigation(tablist);
  assert(tabButtons[0].attributes['aria-selected'] === 'true' && tabButtons[0].tabIndex === 0
    && tabPanes[0].hidden === false, 'Tab navigation initializes selected tab and panel state');
  let prevented = false;
  tabButtons[0].handlers.keydown({ key: 'ArrowRight', preventDefault() { prevented = true; } });
  assert(prevented && tabButtons[1].attributes['aria-selected'] === 'true'
    && tabButtons[1].tabIndex === 0 && tabPanes[1].hidden === false
    && tabPanes[0].hidden === true && tabButtons[1].focused,
    'Arrow navigation keeps focus, selection and visible tab panel synchronized');
  tabButtons[1].handlers.keydown({ key: 'End', preventDefault() {} });
  assert(tabButtons[3].attributes['aria-selected'] === 'true' && tabPanes[3].hidden === false,
    'End key selects the last settings tab');

  // 4. Verify types.d.ts matches Rust DTOs
  const dtsPath = path.join(__dirname, 'src', 'types.d.ts');
  assert(fs.existsSync(dtsPath), 'types.d.ts exists');
  const dtsContent = fs.readFileSync(dtsPath, 'utf8');

  assert(dtsContent.includes('interface AudioDevice'), 'AudioDevice type declared');
  assert(dtsContent.includes('interface ServiceStatus'), 'ServiceStatus type declared');
  assert(dtsContent.includes('interface LiveAudioConfig'), 'LiveAudioConfig type declared');
  assert(dtsContent.includes('interface ObsOverlayInfo'), 'ObsOverlayInfo type declared');
  assert(dtsContent.includes('interface DiagnosticsExportResult'), 'DiagnosticsExportResult type declared');
  assert(dtsContent.includes('interface SubtitleCue'), 'SubtitleCue type declared');
  assert(dtsContent.includes('rnnoise_enabled: boolean;')
    && dtsContent.includes('acoustic_events_enabled: boolean;'),
    'LiveAudioConfig declares persisted optional audio flags');
  assert(dtsContent.includes('asr_backend: string;'), 'LiveAudioConfig types declare asr_backend');
  assert(dtsContent.includes('live_partial_subtitles_enabled: boolean;'),
    'LiveAudioConfig declares the live partial subtitles flag');
  for (const key of ['lbl_live_partial_subtitles', 'hint_live_partial_subtitles']) {
    assert((i18nContent.match(new RegExp(`${key}:`, 'g')) || []).length === 2,
      `${key} is localized in Spanish and English`);
  }
  for (const key of ['lbl_rnnoise_enabled', 'hint_rnnoise_enabled', 'lbl_acoustic_events_enabled', 'hint_acoustic_events_enabled']) {
    assert((i18nContent.match(new RegExp(`${key}:`, 'g')) || []).length === 2,
      `${key} is localized in Spanish and English`);
  }
  assert(/hint_acoustic_events_enabled:[\s\S]*?OpenCohost[\s\S]*?metadata/.test(i18nContent)
    && /hint_rnnoise_enabled:[\s\S]*?RNNoise[\s\S]*?VAD/.test(i18nContent)
    && !i18nContent.includes('not active yet')
    && !i18nContent.includes('integration is complete'),
    'help copy describes active RNNoise and metadata-only events without interim wording');

  const appJsContent = fs.readFileSync(path.join(__dirname, 'src', 'app.js'), 'utf8');
  assert(/status\.ws_port === 0[\s\S]{0,120}t\('ws_off'\)/.test(appJsContent),
    'WS pill shows an off label instead of port 0 when OBS is disabled');
  assert((i18nContent.match(/ws_off:/g) || []).length === 2,
    'ws_off is localized in Spanish and English');

  const baseline = { model_size: 'small', device: 'cpu', ws_port: 8765, subtitle_style: 'default' };
  assert(!runtimeSettingsChanged(baseline, { ...baseline, subtitle_style: 'neon' }),
    'style-only change does not request a service restart');
  assert(runtimeSettingsChanged(baseline, { ...baseline, ws_port: 9000 }),
    'WebSocket port change requires a confirmed service restart');
  assert(runtimeSettingsChanged(baseline, { ...baseline, model_size: 'turbo' }),
    'ASR model change requires a confirmed service restart');
  assert(runtimeSettingsChanged(baseline, { ...baseline, rnnoise_enabled: true })
    && runtimeSettingsChanged(baseline, { ...baseline, acoustic_events_enabled: true }),
    'both optional audio settings require a confirmed service restart');
  assert(runtimeSettingsChanged(baseline, { ...baseline, live_partial_subtitles_enabled: true }),
    'the live partial subtitles flag requires a confirmed service restart');
  const indexHtml = fs.readFileSync(path.join(__dirname, 'src', 'index.html'), 'utf8');
  assert(indexHtml.includes('id="check-mcp-style-enabled"') && indexHtml.includes('data-i18n="lbl_mcp_style_enabled"')
    && indexHtml.includes('data-i18n="hint_mcp_style_enabled"'),
    'the MCP style opt-in switch lives in the Advanced tab with its label and hint');
  assert(indexHtml.includes('id="check-mcp-system-enabled"') && indexHtml.includes('data-i18n="lbl_mcp_system_enabled"')
    && indexHtml.includes('data-i18n="hint_mcp_system_enabled"'),
    'the MCP system opt-in switch lives in the Advanced tab with its label and hint');
  assert((i18nContent.match(/lbl_mcp_style_enabled:/g) || []).length === 2
    && (i18nContent.match(/hint_mcp_style_enabled:/g) || []).length === 2,
    'the MCP style opt-in label and hint are localized in Spanish and English');
  assert((i18nContent.match(/lbl_mcp_system_enabled:/g) || []).length === 2
    && (i18nContent.match(/hint_mcp_system_enabled:/g) || []).length === 2,
    'the MCP system opt-in label and hint are localized in Spanish and English');
  assert(/cfg\.mcp_style_enabled\s*=\s*elements\.checkMcpStyleEnabled\.checked/.test(appJsContent)
    && /checkMcpStyleEnabled[\s\S]{0,80}\.checked\s*=\s*config\.mcp_style_enabled/.test(appJsContent),
    'the MCP style switch is read into and populated from the settings form');
  assert(/cfg\.mcp_system_enabled\s*=\s*elements\.checkMcpSystemEnabled\.checked/.test(appJsContent)
    && /checkMcpSystemEnabled[\s\S]{0,80}\.checked\s*=\s*config\.mcp_system_enabled/.test(appJsContent),
    'the MCP system switch is read into and populated from the settings form');
  const externalClean = externalConfigUpdate({ hasUnsavedEdits: false, restartRequired: false, isRunning: true });
  assert(externalClean.refreshForm === true && externalClean.messageKey === 'toast_config_changed',
    'an external config change refreshes a form without unsaved edits');
  const externalDirty = externalConfigUpdate({ hasUnsavedEdits: true, restartRequired: false, isRunning: true });
  assert(externalDirty.refreshForm === false && externalDirty.messageKey === 'toast_config_changed_pending',
    'an external config change keeps unsaved form edits and says so');
  assert(externalConfigUpdate({ hasUnsavedEdits: false, restartRequired: true, isRunning: true }).messageKey === 'toast_config_changed_restart',
    'a restart-only external change asks for a service restart while running');
  assert(externalConfigUpdate({ hasUnsavedEdits: false, restartRequired: true, isRunning: false }).messageKey === 'toast_config_changed',
    'a restart-only external change asks for nothing while the service is stopped');
  assert((i18nContent.match(/toast_config_changed(_pending|_restart)?:/g) || []).length === 6,
    'external config change messages are localized in Spanish and English');
  const flagCalls = [];
  const flagApplied = await persistAndApplyRuntimeConfig({
    before: baseline,
    after: { ...baseline, acoustic_events_enabled: true },
    isRunning: true,
    confirmRestart: () => { flagCalls.push('confirm'); return true; },
    save: async () => flagCalls.push('save'),
    restart: async () => { flagCalls.push('restart'); return { is_running: true }; },
  });
  assert(flagApplied.saved && flagApplied.restarted && flagCalls.join(',') === 'confirm,save,restart',
    'acoustic event preference confirms before save and restarts only after save');
  flagCalls.length = 0;
  try {
    await persistAndApplyRuntimeConfig({
      before: baseline,
      after: { ...baseline, rnnoise_enabled: true },
      isRunning: true,
      confirmRestart: () => { flagCalls.push('confirm'); return true; },
      save: async () => { flagCalls.push('save'); throw new Error('disk unavailable'); },
      restart: async () => { flagCalls.push('restart'); },
    });
  } catch (_) {}
  assert(flagCalls.join(',') === 'confirm,save',
    'persistence failure prevents restart and leaves active configuration untouched');
  const calls = [];
  const applied = await persistAndApplyRuntimeConfig({
    before: baseline,
    after: { ...baseline, ws_port: 9000 },
    isRunning: true,
    confirmRestart: () => { calls.push('confirm'); return true; },
    save: async () => calls.push('save'),
    restart: async () => { calls.push('restart'); return { is_running: true, ws_port: 9000 }; },
  });
  assert(applied.restarted && calls.join(',') === 'confirm,save,restart',
    'confirmed runtime changes persist before restarting the service');
  calls.length = 0;
  const declined = await persistAndApplyRuntimeConfig({
    before: baseline,
    after: { ...baseline, ws_port: 9000 },
    isRunning: true,
    confirmRestart: () => { calls.push('confirm'); return false; },
    save: async () => calls.push('save'),
    restart: async () => calls.push('restart'),
  });
  assert(!declined.saved && calls.join(',') === 'confirm',
    'declining runtime restart leaves settings unsaved');
  const restartFailed = await persistAndApplyRuntimeConfig({
    before: baseline,
    after: { ...baseline, ws_port: 9000 },
    isRunning: true,
    confirmRestart: () => true,
    save: async () => calls.push('saved-after-failure'),
    restart: async () => { throw new Error('worker unavailable'); },
  });
  assert(restartFailed.saved && restartFailed.restartError instanceof Error,
    'restart failure is reported separately from the persisted settings');

  const savedBaseline = { ...baseline };
  const balancedProfile = {
    id: 'balanced', device: 'cuda', model_size: 'small', silence_timeout: 0.5,
    subtitle_backlog_policy: 'auto', subtitle_max_live_delay_sec: 10, subtitle_catchup_interval_sec: 1.5,
  };
  const profileSelect = {
    value: 'balanced',
    addEventListener(event, callback) { this.changeHandler = callback; },
  };
  let profileDraft = {
    ...savedBaseline,
    audio_device: { id: 'usb', name: 'USB mic' },
    subtitle_style: 'neon',
    rnnoise_enabled: false,
    acoustic_events_enabled: true,
  };
  bindProfileSelection(profileSelect, {
    getDraft: () => profileDraft,
    setDraft: (draft) => { profileDraft = draft; },
    getPresets: () => [balancedProfile],
    onChange: () => {},
  });
  profileSelect.changeHandler();
  assert(savedBaseline.device === 'cpu', 'profile selection keeps the saved config baseline immutable');
  assert(profileDraft.audio_device.id === 'usb' && profileDraft.subtitle_style === 'neon',
    'profile selection preserves unrelated unsaved settings in the draft');
  assert(profileDraft.rnnoise_enabled === false && profileDraft.acoustic_events_enabled === true,
    'profiles that omit optional audio flags preserve the current draft values');
  const explicitProfile = runtimeSettings.applyProfilePreset(
    { ...profileDraft, rnnoise_enabled: false, acoustic_events_enabled: true },
    { ...balancedProfile, rnnoise_enabled: true, acoustic_events_enabled: false },
  );
  assert(explicitProfile.rnnoise_enabled === true && explicitProfile.acoustic_events_enabled === false,
    'profiles apply explicit true and false optional audio values');
  assert(appSource.includes('getDraft: () => collectFormData()'),
    'profile selection captures current form edits before applying preset values');
  const profileCalls = [];
  const profileApplied = await persistAndApplyRuntimeConfig({
    before: savedBaseline,
    after: profileDraft,
    isRunning: true,
    confirmRestart: () => { profileCalls.push('confirm'); return true; },
    save: async () => profileCalls.push('save'),
    restart: async () => { profileCalls.push('restart'); return { is_running: true }; },
  });
  assert(profileApplied.restarted && profileCalls.join(',') === 'confirm,save,restart',
    'selecting a profile then saving uses the saved baseline and restarts the live service');
  assert(typeof runtimeSettings.hardwareStatusLabel === 'function'
    && runtimeSettings.hardwareStatusLabel({ is_running: true, model_size: 'base', device: 'cpu', active_device: 'USB mic' })
      .includes('USB mic'),
    'runtime hardware status shows the selected audio input');
  const monitorCalls = [];
  const monitorSaved = await persistAndApplyRuntimeConfig({
    before: baseline,
    after: { ...baseline, ws_port: 9000 },
    isRunning: false,
    monitorMode: true,
    confirmRestart: () => { monitorCalls.push('confirm'); return true; },
    save: async () => monitorCalls.push('save'),
    restart: async () => monitorCalls.push('restart'),
  });
  assert(monitorSaved.saved && !monitorSaved.restarted && monitorSaved.deferred === true
    && monitorCalls.join(',') === 'save',
    'Monitor Mode saves runtime settings as deferred without prompting or restarting the foreign service');
  const styleOnlyInMonitor = await persistAndApplyRuntimeConfig({
    before: baseline,
    after: { ...baseline, subtitle_style: 'neon' },
    isRunning: false,
    monitorMode: true,
    confirmRestart: () => true,
    save: async () => {},
    restart: async () => {},
  });
  assert(styleOnlyInMonitor.saved && !styleOnlyInMonitor.deferred,
    'Monitor Mode does not flag style-only changes as deferred');
  const { monitorFeedAction, monitorFeedText } = runtimeSettings;
  assert(typeof monitorFeedAction === 'function'
    && monitorFeedAction({ monitor_mode: true, ws_port: 8766 }, null) === 'open'
    && monitorFeedAction({ monitor_mode: true, ws_port: 8766 }, 8766) === 'keep'
    && monitorFeedAction({ monitor_mode: true, ws_port: 8767 }, 8766) === 'open'
    && monitorFeedAction({ monitor_mode: false, ws_port: 8766 }, 8766) === 'close'
    && monitorFeedAction({ is_running: true, ws_port: 8765 }, null) === 'keep',
    'read-only feed opens on the foreign port, follows port changes, and closes outside Monitor Mode');
  assert(typeof monitorFeedText === 'function'
    && monitorFeedText('{"text":"hola","style":"default"}') === 'hola'
    && monitorFeedText('{"type":"hello","app":"liveaudio","proto":1}') === ''
    && monitorFeedText('{"type":"theme","tokens":{}}') === ''
    && monitorFeedText('not json') === '',
    'read-only feed shows subtitle text and ignores hello, theme and malformed frames');
  assert(htmlContent.includes('id="monitor-banner"'), 'Monitor Mode banner element exists');
  assert(appSource.includes('new WebSocket(') && appSource.includes('currentStatus.monitor_mode'),
    'desktop opens a read-only WebSocket feed and guards the toggle in Monitor Mode');
  assert(typeof runtimeSettings.audioStateKey === 'function'
    && runtimeSettings.audioStateKey({ is_running: true, audio_state: 'standby' }) === 'audio_standby'
    && runtimeSettings.audioStateKey({ is_running: true, audio_state: 'reconnecting' }) === 'audio_reconnecting'
    && runtimeSettings.audioStateKey({ is_running: true, audio_state: 'capturing' }) === ''
    && runtimeSettings.audioStateKey({ is_running: false, audio_state: 'standby' }) === '',
    'audio standby and reconnect states are surfaced only while the service runs');
  assert(typeof runtimeSettings.sessionFailureLabel === 'function'
    && runtimeSettings.sessionFailureLabel({ session_error: 'Session output failed: 1 write(s), 0 rejected' })
      .includes('1 write'),
    'session status exposes output failures without requiring diagnostic logs');
  assert(runtimeSettings.isServiceTransitioning(true, 'stopped'),
    'isServiceTransitioning is true when isPending is true');
  assert(runtimeSettings.isServiceTransitioning(false, 'starting'),
    'isServiceTransitioning is true when asrState is starting');
  assert(runtimeSettings.isServiceTransitioning(false, 'stopping'),
    'isServiceTransitioning is true when asrState is stopping');
  assert(runtimeSettings.isServiceTransitioning(false, 'Descargando (45%)'),
    'isServiceTransitioning is true when downloading model');
  assert(!runtimeSettings.isServiceTransitioning(false, 'listening'),
    'isServiceTransitioning is false when listening');
  assert(!runtimeSettings.isServiceTransitioning(false, 'stopped'),
    'isServiceTransitioning is false when stopped and not pending');

  // 5. Verify i18n module, Fran dog logo and bilingual profiles
  const logoPath = path.join(__dirname, 'src', 'assets', 'logo.png');
  assert(fs.existsSync(logoPath), 'Fran dog logo asset exists at desktop/src/assets/logo.png');
  assert(htmlContent.includes('src="assets/logo.png"'), 'Fran dog logo is embedded in index.html');
  assert(htmlContent.includes('id="select-ui-lang"'), 'UI language select dropdown exists in index.html');

  const i18nPath = path.join(__dirname, 'src', 'i18n.js');
  assert(fs.existsSync(i18nPath), 'i18n.js exists');
  const { t, setLanguage, getLanguage, getProfilePresetData, PROFILE_I18N, applyTranslations } = await import('./src/i18n.js');

  const monitorKeys = ['btn_monitor', 'status_monitor', 'monitor_banner', 'monitor_pid_unknown', 'toast_config_saved_deferred'];
  for (const lang of ['es', 'en']) {
    setLanguage(lang);
    assert(monitorKeys.every((key) => t(key) && t(key) !== key), `Monitor Mode copy exists in ${lang}`);
    const banner = typeof runtimeSettings.monitorBannerText === 'function'
      ? runtimeSettings.monitorBannerText({ monitor_mode: true, foreign_service_pid: 4242 }, t)
      : '';
    assert(banner.includes('4242') && !banner.includes('OpenCohost'),
      `Monitor Mode banner names the owner PID without assuming the owner app (${lang})`);
    assert(runtimeSettings.monitorBannerText({ monitor_mode: true, foreign_service_pid: null }, t)
      .includes(t('monitor_pid_unknown'))
      && runtimeSettings.monitorBannerText({ monitor_mode: false }, t) === '',
    `Monitor Mode banner handles an unknown PID and is empty outside Monitor Mode (${lang})`);
  }

  setLanguage('es');
  assert(getLanguage() === 'es', 'language setter activates Spanish');
  assert(t('tab_setup') === 'Inicio' && t('tab_files') === 'Archivos', 'Spanish settings navigation labels exist');
  assert(t('btn_start') === 'Iniciar Transcripción', 'Spanish translation for btn_start');
  assert(t('status_stopped') === 'Detenido', 'Spanish translation for status_stopped');
  assert(t('audio_standby') === 'Esperando micrófono' && t('audio_reconnecting') === 'Reconectando audio…',
    'Spanish audio capture state labels exist');
  setLanguage('en');
  assert(t('audio_standby') === 'Waiting for microphone' && t('audio_reconnecting') === 'Reconnecting audio…',
    'English audio capture state labels exist');
  setLanguage('es');
  const ariaLabelElement = {
    attributes: { 'data-i18n-aria-label': 'settings_tabs_label' },
    getAttribute(name) { return this.attributes[name]; },
    setAttribute(name, value) { this.attributes[name] = value; },
  };
  const previousDocument = globalThis.document;
  globalThis.document = {
    querySelectorAll(selector) { return selector === '[data-i18n-aria-label]' ? [ariaLabelElement] : []; },
  };
  applyTranslations('es');
  assert(ariaLabelElement.attributes['aria-label'] === 'Configuración',
    'Spanish accessible name for settings tablist is translated');
  applyTranslations('en');
  assert(ariaLabelElement.attributes['aria-label'] === 'Settings',
    'English accessible name for settings tablist is translated');
  if (previousDocument === undefined) delete globalThis.document;
  else globalThis.document = previousDocument;

  const balancedEs = getProfilePresetData('balanced', 'es');
  assert(balancedEs.label === 'Balanceado' && balancedEs.samplePrompt.length > 0,
    'Spanish balanced profile preset has label and prompt suggestion');

  setLanguage('en');
  assert(getLanguage() === 'en', 'language setter activates English');
  assert(t('tab_setup') === 'Setup' && t('tab_files') === 'Files', 'English settings navigation labels exist');
  assert(t('btn_start') === 'Start Transcription', 'English translation for btn_start');
  assert(t('status_stopped') === 'Stopped', 'English translation for status_stopped');

  const balancedEn = getProfilePresetData('balanced', 'en');
  assert(balancedEn.label === 'Balanced' && balancedEn.samplePrompt.length > 0,
    'English balanced profile preset has label and prompt suggestion');

  const gettingStarted = fs.readFileSync(path.join(__dirname, '..', 'docs', 'GETTING_STARTED.md'), 'utf8');
  assert(gettingStarted.includes('carpeta de la sesión actual o la carpeta de salida configurada'),
    'Desktop session path documentation explains the configured output-folder fallback');
  assert(gettingStarted.includes('valores predeterminados actuales')
    && gettingStarted.includes('controles JSONL y WebVTT son configurables'),
    'Desktop file documentation preserves existing sink defaults and user configurability');

  const fastEn = getProfilePresetData('fast', 'en');
  assert(fastEn.label === 'Fast' && fastEn.samplePrompt.length > 0,
    'English fast profile preset has label and prompt suggestion');

  const qualityEn = getProfilePresetData('quality', 'en');
  assert(qualityEn.label === 'Quality' && qualityEn.samplePrompt.length > 0,
    'English quality profile preset has label and prompt suggestion');

  const stableEn = getProfilePresetData('stable_streaming', 'en');
  assert(stableEn.label === 'Stable Streaming' && stableEn.samplePrompt.length > 0,
    'English stable streaming profile preset has label and prompt suggestion');

  // 6. Verify version 1.6.0 consistency across workspace and manifests
  const rootCargo = fs.readFileSync(path.join(__dirname, '..', 'Cargo.toml'), 'utf8');
  assert(rootCargo.includes('version = "1.6.0"'), 'Root Cargo.toml has version 1.6.0');
  const tauriConf = JSON.parse(fs.readFileSync(path.join(__dirname, 'src-tauri', 'tauri.conf.json'), 'utf8'));
  assert(tauriConf.version === '1.6.0', 'tauri.conf.json has version 1.6.0');
  // NSIS locates the previous install via HKCU\Software\<publisher>\<productName>;
  // every released build used "franguh", so changing it breaks upgrades and updates.
  assert(tauriConf.bundle.publisher === 'franguh',
    'bundle.publisher stays "franguh" so upgrades find the previous install');
  const csp = tauriConf.app.security.csp;
  assert(typeof csp === 'string' && csp.length > 0, 'webview CSP is set (not null)');
  const cspDirectives = Object.fromEntries(String(csp).split(';').map((d) => d.trim()).filter(Boolean).map((d) => { const [name, ...values] = d.split(/\s+/); return [name, values]; }));
  assert(cspDirectives['default-src']?.join(' ') === "'self'", "CSP default-src is 'self'");
  assert(cspDirectives['script-src']?.join(' ') === "'self'", "CSP script-src is 'self' only (no inline or eval)");
  assert(cspDirectives['object-src']?.join(' ') === "'none'", "CSP object-src is 'none'");
  assert(cspDirectives['connect-src']?.includes('ipc:') && cspDirectives['connect-src']?.includes('http://ipc.localhost'), 'CSP connect-src allows Tauri IPC');
  assert(cspDirectives['connect-src']?.includes('ws://127.0.0.1:*'), 'CSP connect-src allows the loopback feed WebSocket');
  assert(cspDirectives['img-src']?.includes('data:'), 'CSP img-src allows data: images');
  assert(cspDirectives['style-src']?.includes("'unsafe-inline'"), 'CSP style-src allows the inline style attributes in index.html');
  assert(tauriConf.bundle.category === 'Music', 'Tauri bundle category uses a supported audio-related category');
  const expectedResourceMap = {
    '../../LICENSE': 'LICENSE',
    '../../THIRD_PARTY_NOTICES.md': 'THIRD_PARTY_NOTICES.md',
    '../../assets/*': 'assets',
  };
  assert(JSON.stringify(tauriConf.bundle.resources) === JSON.stringify(expectedResourceMap),
    'Tauri maps the third-party notice and direct runtime files');
  const platformConf = (name) => JSON.parse(fs.readFileSync(path.join(__dirname, 'src-tauri', name), 'utf8'));
  assert(platformConf('tauri.windows.conf.json').bundle.resources['../../packaging/onnxruntime/windows-x64/onnxruntime.dll'] === 'onnxruntime/windows-x64/onnxruntime.dll',
    'Windows bundle maps the ONNX runtime dll');
  assert(platformConf('tauri.linux.conf.json').bundle.resources['../../packaging/onnxruntime/linux-x64/libonnxruntime.so.1.24.4'] === 'onnxruntime/linux-x64/libonnxruntime.so.1.24.4',
    'Linux bundle maps the ONNX runtime binary');
  assert(platformConf('tauri.windows.conf.json').bundle.resources['../../packaging/onnxruntime/windows-x64/DirectML.dll'] === 'onnxruntime/windows-x64/DirectML.dll'
    && platformConf('tauri.windows.conf.json').bundle.resources['../../packaging/onnxruntime/windows-x64/DirectML-LICENSE.txt'] === 'onnxruntime/windows-x64/DirectML-LICENSE.txt',
  'Windows bundle maps the DirectML runtime and its license');
  const linuxResources = { ...tauriConf.bundle.resources, ...platformConf('tauri.linux.conf.json').bundle.resources };
  assert(!Object.keys(linuxResources).some((source) => source.includes('windows-x64')),
    'Linux bundle (base merged with the Linux platform config) does not receive Windows-only ONNX runtime files');
  assert(htmlContent.includes('id="btn-runtime-repair"'), 'ASR runtime repair action exists');
  assert(htmlContent.includes('id="btn-runtime-cancel"'), 'ASR setup cancellation action exists');
  assert(jsContent.includes("listen('runtime_setup_progress'"), 'Runtime setup progress is forwarded to the UI');
  assert(jsContent.includes('status.asr_progress'), 'Worker model progress is read from typed service status');
  assert(jsContent.includes("phase: 'downloading'"), 'Model download progress reaches the shared progress UI');
  assert(jsContent.includes("'starting_worker', 'downloading'"), 'Cancellation stays enabled through model download and readiness');
  assert(jsContent.includes("removeAttribute('value')"), 'Unknown byte totals render as indeterminate progress');
  assert(dtsContent.includes('asr_progress: AsrProgress | null'), 'Service status types carry optional model progress');
  assert(htmlContent.includes('id="runtime-setup-meter"'), 'Model progress can use determinate or indeterminate HTML progress');
  assert(jsContent.includes("invoke('cancel_runtime_setup'"), 'Runtime setup cancellation command is wired');
  assert(jsContent.includes("config.device || 'cpu'"), 'UI falls back to CPU only when a saved device is absent');
  assert(/<option value="cpu" id="device-cpu"[^>]*selected/.test(htmlContent), 'CPU is selected before saved configuration loads');
  assert(Object.entries(tauriConf.bundle.resources).some(([source, target]) =>
    source === '../../assets/*' && target === 'assets'),
  'Assets are mapped in bundle resources');
  assert(JSON.stringify(tauriConf.bundle.windows.nsis.languages) === JSON.stringify(['English', 'SpanishInternational']),
    'NSIS installer languages use installed NSIS language identifiers for English and es-ES');
  assert(tauriConf.bundle.windows.nsis.displayLanguageSelector === false,
    'NSIS installer keeps the configured operating-system language selection behavior');
  assert(tauriConf.bundle.windows.nsis.installerHooks === 'hooks.nsh',
    'NSIS installer hooks script is configured in tauri.conf.json bundle windows nsis');

  // Capability scope: the default capability is limited to the main window. The explicit
  // allow-emit entry is unused (core:default still expands to it; full revocation is deferred).
  const capability = JSON.parse(fs.readFileSync(path.join(__dirname, 'src-tauri', 'capabilities', 'default.json'), 'utf8'));
  assert(JSON.stringify(capability.windows) === JSON.stringify(['main']),
    'Default capability is scoped to the main window only');
  assert(!capability.permissions.includes('core:event:allow-emit'),
    'Default capability no longer lists the unused core:event:allow-emit entry');
  // onCloseRequested finishes an unprevented close by calling window.destroy(); without this
  // permission the call is rejected and the window can never be closed.
  assert(capability.permissions.includes('core:window:allow-destroy'),
    'Default capability grants core:window:allow-destroy for the onCloseRequested close path');
  assert(!/\bemit\s*\(/.test(jsContent), 'Frontend does not call emit, so the emit permission is unnecessary');

  // The event panel ready line is translated through i18n, not hard-coded Spanish
  const i18nSource = fs.readFileSync(path.join(__dirname, 'src', 'i18n.js'), 'utf8');
  assert(!jsContent.includes("'Panel de eventos listo.'"), 'Event panel ready log line is not hard-coded Spanish');
  assert(jsContent.includes("t('log_events_ready')"), 'Event panel ready log line uses the log_events_ready i18n key');
  assert(i18nSource.includes("log_events_ready: 'Panel de eventos listo.'") && i18nSource.includes("log_events_ready: 'Event panel ready.'"),
    'i18n defines log_events_ready in both Spanish and English');

  // Verify NSIS hooks script content and legacy migration coverage
  const hooksPath = path.join(__dirname, 'src-tauri', 'hooks.nsh');
  assert(fs.existsSync(hooksPath), 'hooks.nsh exists in desktop/src-tauri');
  const hooksContent = fs.readFileSync(hooksPath, 'utf8');
  assert(hooksContent.includes('!macro NSIS_HOOK_PREINSTALL'), 'hooks.nsh defines NSIS_HOOK_PREINSTALL');
  assert(hooksContent.includes('IfFileExists "$R2\\liveaudio-desktop.exe"'),
    'Legacy predicate treats an install directory that holds liveaudio-desktop.exe as modern');
  assert(hooksContent.includes('${If} $R9 >= 5'),
    'Legacy predicate treats any major.minor >= 1.5 as modern, so 1.6.0 without MainBinaryName is not purged');
  assert(hooksContent.includes('SetRegView 64') && hooksContent.includes('SetRegView 32'),
    'hooks.nsh inspects both 64-bit and 32-bit registry views');
  assert(hooksContent.includes('HKCU') && hooksContent.includes('HKLM'),
    'hooks.nsh inspects both HKCU and HKLM uninstall registries');
  assert(hooksContent.includes('CheckAndCleanLegacyRegistryKey HKCU "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\LiveAudio"'),
    'hooks.nsh inspects HKCU LiveAudio uninstall registry key in 64-bit view');
  assert(hooksContent.includes('CleanInstallLocationJsonFile'),
    'hooks.nsh inspects and cleans install_location.json from legacy Python launchers');
  assert(hooksContent.includes('installed.json') && hooksContent.includes('liveaudio.exe'),
    'hooks.nsh inspects legacy Python installed.json and liveaudio.exe signatures');
  assert(hooksContent.includes('PurgeLegacyPythonDirectory') && hooksContent.includes('PurgeLegacyShortcuts'),
    'hooks.nsh purges deprecated Python files and legacy shortcuts');
  const postUninstallMatch = hooksContent.match(/!macro NSIS_HOOK_POSTUNINSTALL([\s\S]*?)!macroend/);
  const postUninstall = postUninstallMatch ? postUninstallMatch[1] : '';
  const outsidePostUninstall = hooksContent.replace(postUninstall, '');
  assert(hooksContent.includes('$APPDATA\\LiveAudio') && !outsidePostUninstall.includes('RMDir /r "$APPDATA\\LiveAudio"'),
    'hooks.nsh protects and preserves user configuration and data in %APPDATA%\\LiveAudio outside the opt-in uninstall hook');
  assert(postUninstall.includes('$DeleteAppDataCheckboxState = 1') && postUninstall.includes('$UpdateMode <> 1'),
    'POSTUNINSTALL removes LiveAudio data only when the delete-app-data box is ticked and not during an update');
  assert(postUninstall.includes('SetShellVarContext current')
    && postUninstall.includes('RMDir /r "$APPDATA\\LiveAudio"')
    && postUninstall.includes('RMDir /r "$LOCALAPPDATA\\LiveAudio"'),
    'POSTUNINSTALL removes the fixed default %APPDATA% and %LOCALAPPDATA% LiveAudio folders');
  assert(!postUninstall.includes('LIVEAUDIO_HOME'), 'POSTUNINSTALL never deletes a LIVEAUDIO_HOME-redirected path');
  assert(hooksContent.includes('ExecWait'), 'hooks.nsh executes legacy uninstaller silently');
  assert(hooksContent.includes('/VERYSILENT') && hooksContent.includes('/S'),
    'hooks.nsh supports both InnoSetup and NSIS silent uninstaller switches');
  assert(hooksContent.includes('liveaudio-desktop.exe'),
    'hooks.nsh identifies modern Rust desktop client to prevent false-positive uninstallation');

  assert(t('app_version').includes('v1.6.0'), 'i18n app_version includes v1.6.0');

  // 7. Verify Accessible Modal System & 3rd Loading State Button
  assert(htmlContent.includes('id="modal-container"'), 'Accessible modal container element exists in index.html');
  assert(htmlContent.includes('id="modal-btn-confirm"'), 'Modal confirm button element exists');
  assert(htmlContent.includes('id="modal-btn-cancel"'), 'Modal cancel button element exists');
  assert(cssContent.includes('.btn-toggle-service.loading'), 'Loading button state CSS class exists');
  assert(cssContent.includes('.modal-backdrop'), 'Modal backdrop overlay CSS class exists');
  assert(t('modal_title_confirm').length > 0, 'Spanish modal confirm title translation exists');
  assert(t('btn_starting').length > 0, 'Spanish button starting state translation exists');
  setLanguage('en');
  assert(t('modal_title_confirm').length > 0, 'English modal confirm title translation exists');
  assert(t('btn_starting').length > 0, 'English button starting state translation exists');

  // 8. Verify Event Logger Panel and Uncluttered Action Bar
  assert(htmlContent.includes('id="card-logger"'), 'Event Logger card element exists');
  assert(htmlContent.includes('id="logger-console"'), 'Event Logger console element exists');
  assert(htmlContent.includes('id="btn-clear-logs"'), 'Event Logger clear button exists');
  assert(htmlContent.includes('id="btn-copy-logs"'), 'Event Logger copy button exists');
  assert(htmlContent.includes('id="logger-count-badge"'), 'Event Logger counter badge exists');
  assert(htmlContent.includes('class="tabs-footer"'), 'Settings tabs footer exists');
  assert(cssContent.includes('.logger-card'), 'Logger card CSS styles exist');
  assert(cssContent.includes('.logger-console'), 'Logger console CSS styles exist');
  assert(cssContent.includes('.tabs-footer'), 'Tabs footer CSS styles exist');

  // Verify that master-action-bar no longer contains save or reset buttons
  const masterActionBarHtml = htmlContent.match(/<div class="master-action-bar">([\s\S]*?)<\/div>/)?.[1] || '';
  assert(masterActionBarHtml.includes('id="btn-toggle-service"'), 'Master action bar contains toggle service button');
  assert(!masterActionBarHtml.includes('id="btn-save-config"'), 'Master action bar no longer contains save button');
  assert(!masterActionBarHtml.includes('id="btn-reset-config"'), 'Master action bar no longer contains reset button');
  assert(!masterActionBarHtml.includes('id="btn-runtime-repair"'), 'Master action bar no longer contains runtime repair button');

  // Verify translations for Event Logger
  setLanguage('es');
  assert(t('title_event_logger').length > 0, 'Spanish Event Logger title translation exists');
  assert(t('btn_clear_logs').length > 0, 'Spanish Event Logger clear translation exists');
  assert(t('btn_copy_logs').length > 0, 'Spanish Event Logger copy translation exists');
  setLanguage('en');
  assert(t('title_event_logger').length > 0, 'English Event Logger title translation exists');
  assert(t('btn_clear_logs').length > 0, 'English Event Logger clear translation exists');
  assert(t('btn_copy_logs').length > 0, 'English Event Logger copy translation exists');

  // Verify collapse/minimize toggle button for Event Logger
  assert(htmlContent.includes('id="btn-toggle-logger"'), 'Event Logger toggle button exists in index.html');
  assert(cssContent.includes('.logger-card.collapsed'), 'Logger card collapsed CSS exists');
  assert(cssContent.includes('.logger-card.collapsed .logger-console'), 'Logger console hide CSS exists');
  assert(cssContent.includes('.form-group.device-dimmed'), 'Form group device-dimmed CSS exists');
  assert(htmlContent.includes('id="group-cpu-threads"'), 'CPU threads container has group-cpu-threads ID');

  // Verify VAD pill is in main header status area and not inside status-detail-items
  const headerStatusHtml = htmlContent.match(/<div class="header-status">([\s\S]*?)<\/details>/)?.[1] || '';
  const detailItemsHtml = htmlContent.match(/<div class="status-detail-items">([\s\S]*?)<\/div>/)?.[1] || '';
  assert(headerStatusHtml.includes('id="pill-vad"'), 'VAD pill is located in the main header status area');
  assert(!detailItemsHtml.includes('id="pill-vad"'), 'VAD pill is not hidden inside status details');

  // Verify translations for toggle logger and cuda device hint
  setLanguage('es');
  assert(t('btn_toggle_logger_hide') === 'Ocultar', 'Spanish Event Logger hide translation exists');
  assert(t('btn_toggle_logger_show') === 'Mostrar', 'Spanish Event Logger show translation exists');
  assert(t('hint_cpu_threads_cuda').length > 0, 'Spanish CUDA CPU threads hint translation exists');
  setLanguage('en');
  assert(t('btn_toggle_logger_hide') === 'Hide', 'English Event Logger hide translation exists');
  assert(t('btn_toggle_logger_show') === 'Show', 'English Event Logger show translation exists');
  assert(t('hint_cpu_threads_cuda').length > 0, 'English CUDA CPU threads hint translation exists');

  // Verify accessibility attributes and default collapsed state on toggle button
  assert(htmlContent.includes('id="card-logger"') && htmlContent.includes('card logger-card collapsed'), 'Event Logger card is collapsed by default');
  assert(htmlContent.includes('id="btn-toggle-logger"') && htmlContent.includes('aria-expanded="false"') && htmlContent.includes('aria-controls="logger-console"'), 'Event Logger toggle button has accessible aria attributes indicating collapsed state');
  assert(htmlContent.includes('data-i18n="btn_toggle_logger_show"'), 'Event Logger toggle button shows expand action by default');
  assert(appSource.includes('isLoggerCollapsed = true;'), 'Logger is collapsed by default in app.js');

  // 9. Verify Stream-Chat Subtitle Feed Component & Interactions
  assert(htmlContent.includes('id="card-subtitles-feed"'), 'Subtitle Feed card element exists in index.html');
  assert(htmlContent.includes('id="subtitles-feed-container"'), 'Subtitle Feed container element exists');
  assert(htmlContent.includes('id="feed-empty-state"'), 'Subtitle Feed empty state placeholder exists');
  assert(htmlContent.includes('id="feed-count-badge"'), 'Subtitle Feed count badge exists');
  assert(htmlContent.includes('id="btn-test-subtitle"'), 'Subtitle Feed test button exists');
  assert(htmlContent.includes('id="btn-clear-feed"'), 'Subtitle Feed clear button exists');
  assert(htmlContent.includes('id="feed-action-menu"'), 'Subtitle Feed action dropdown menu exists');
  assert(htmlContent.includes('data-action="blacklist"')
    && htmlContent.includes('data-action="prompt"')
    && htmlContent.includes('data-action="copy"'),
    'Subtitle Feed dropdown includes blacklist, prompt, and copy actions');

  // Verify CSS styles for Stream-Chat Subtitle Feed
  assert(cssContent.includes('.feed-card'), 'Feed card CSS styles exist');
  assert(cssContent.includes('.feed-container'), 'Feed container CSS styles exist');
  assert(cssContent.includes('.feed-item'), 'Feed item CSS styles exist');
  assert(cssContent.includes('.feed-item-time'), 'Feed item timestamp CSS styles exist');
  assert(cssContent.includes('.feed-item-text'), 'Feed item text CSS styles exist');
  assert(cssContent.includes('.feed-item-blocked'), 'Feed item blocked CSS styles exist');
  assert(cssContent.includes('.feed-item-badge-blocked'), 'Feed item blocked badge CSS styles exist');
  assert(cssContent.includes('.btn-feed-item-menu'), 'Feed item 3-dots menu button CSS styles exist');
  assert(cssContent.includes('.feed-action-dropdown'), 'Feed action dropdown CSS styles exist');
  assert(cssContent.includes('.feed-menu-btn'), 'Feed menu button CSS styles exist');

  // Verify i18n keys for Stream-Chat Subtitle Feed
  setLanguage('es');
  assert(t('title_subtitles_feed') === 'Feed de Subtítulos en Vivo', 'Spanish subtitle feed title translation exists');
  assert(t('feed_empty_hint').length > 0, 'Spanish subtitle feed empty hint exists');
  assert(t('btn_clear_feed') === 'Limpiar', 'Spanish clear feed button translation exists');
  assert(t('menu_add_blacklist') === 'Añadir a Blacklist', 'Spanish add blacklist menu item exists');
  assert(t('menu_add_prompt') === 'Añadir a Vocabulario / Prompt', 'Spanish add prompt menu item exists');
  assert(t('menu_copy_text') === 'Copiar texto', 'Spanish copy text menu item exists');
  assert(t('badge_blocked') === 'Baneado', 'Spanish blocked badge translation exists');
  assert(t('toast_added_to_blacklist').length > 0, 'Spanish add blacklist toast exists');
  assert(t('toast_added_to_prompt').length > 0, 'Spanish add prompt toast exists');
  assert(t('toast_text_copied').length > 0, 'Spanish copy text toast exists');
  assert(t('toast_feed_cleared').length > 0, 'Spanish feed cleared toast exists');

  setLanguage('en');
  assert(t('title_subtitles_feed') === 'Live Subtitles Feed', 'English subtitle feed title translation exists');
  assert(t('feed_empty_hint').length > 0, 'English subtitle feed empty hint exists');
  assert(t('btn_clear_feed') === 'Clear', 'English clear feed button translation exists');
  assert(t('menu_add_blacklist') === 'Add to Blacklist', 'English add blacklist menu item exists');
  assert(t('menu_add_prompt') === 'Add to Initial Prompt', 'English add prompt menu item exists');
  assert(t('menu_copy_text') === 'Copy text', 'English copy text menu item exists');
  assert(t('badge_blocked') === 'Blocked', 'English blocked badge translation exists');
  assert(t('toast_added_to_blacklist').length > 0, 'English add blacklist toast exists');
  assert(t('toast_added_to_prompt').length > 0, 'English add prompt toast exists');
  assert(t('toast_text_copied').length > 0, 'English copy text toast exists');
  assert(t('toast_feed_cleared').length > 0, 'English feed cleared toast exists');

  // Verify JavaScript circular buffer, stick-to-bottom and blacklist logic in app.js
  assert(appSource.includes('MAX_FEED_ITEMS = 120'), 'Feed enforces circular buffer with max 120 elements');
  assert(appSource.includes('formatTimestamp'), 'Timestamp formatting helper exists');
  assert(appSource.includes('addSubtitleToFeed'), 'addSubtitleToFeed helper exists');
  assert(appSource.includes('clearSubtitleFeed'), 'clearSubtitleFeed helper exists');
  assert(appSource.includes('addPhraseToBlacklist'), 'addPhraseToBlacklist helper exists');
  assert(appSource.includes('addTermToInitialPrompt'), 'addTermToInitialPrompt helper exists');
  assert(appSource.includes('openFeedActionMenu') && appSource.includes('closeFeedActionMenu'), 'Accessible dropdown menu handlers exist');
  assert(appSource.includes('container.scrollHeight - container.scrollTop - container.clientHeight'),
    'Smart stick-to-bottom checks scroll offset before inserting new subtitles');
  assert(appSource.includes("listen('subtitle_cue'"), 'Real-time subtitle cues are wired to the feed');
  assert(!/listen\('subtitle_cue'[\s\S]*?appendLogEntry/.test(appSource),
    'Subtitles are exclusive to the stream feed and excluded from event logger console');
  assert(appSource.includes("itemElement.classList.add('feed-item-blocked')"),
    'Blacklisting a phrase strikes through and marks the feed item as blocked');

  // Probar (test subtitle): one feed entry per click, via the subtitle_cue event only
  assert(!/addSubtitleToFeed\(cue\.text/.test(appSource),
    'Probar does not append the returned cue; the subtitle_cue event is the single feed path');
  assert(!appSource.includes('samplePhrases') && !appSource.includes('Math.random() * samplePhrases'),
    'Probar sends one fixed localized test line instead of random marketing phrases');
  assert(appSource.includes("invoke('send_test_subtitle', { text: testText, style })"),
    'Probar sends the localized test text to the backend');
  setLanguage('es');
  assert(t('test_subtitle_text') === 'Subtítulo de prueba de LiveAudio',
    'Spanish test subtitle text is localized');
  setLanguage('en');
  assert(t('test_subtitle_text') === 'LiveAudio test subtitle',
    'English test subtitle text is localized');
  setLanguage('es');

  // Close guard: confirm before closing while this window owns a running session
  assert(/onCloseRequested\(async \(event\) =>/.test(appSource) && /event\.preventDefault\(\);/.test(appSource),
    'Window close is intercepted with onCloseRequested so an owned session can be confirmed first');
  assert(appSource.includes('currentStatus.is_running && !currentStatus.monitor_mode'),
    'Close guard prompts only for a session this window owns, not Monitor Mode');
  assert(/if \(closeConfirmed[\s\S]*?return;/.test(appSource) && /closeConfirmed = true;\s*await appWindow\.close\(\)/.test(appSource),
    'Confirmed close re-enters close without prompting again');
  assert(/if \(ok\) \{[\s\S]*?closeConfirmed = true/.test(appSource) && !appSource.includes('appWindow.destroy'),
    'Cancel keeps the window open and confirm reuses the graceful close path');
  setLanguage('es');
  assert(t('close_confirm_title') === 'Cerrar LiveAudio' && t('close_confirm_action') === 'Detener y cerrar',
    'Spanish close confirmation copy is localized');
  setLanguage('en');
  assert(t('close_confirm_title') === 'Close LiveAudio' && t('close_confirm_action') === 'Stop and close',
    'English close confirmation copy is localized');
  setLanguage('es');
  // Close guard also covers an in-flight NVIDIA install and a style save that failed
  assert(/onCloseRequested[\s\S]*?nvidiaCardState === 'installing'[\s\S]*?styleDirty/.test(appSource),
    'Close guard warns while the NVIDIA runtime installs or a style save failed');
  for (const lang of ['es', 'en']) {
    setLanguage(lang);
    for (const key of ['close_install_message', 'close_install_detail', 'close_install_action',
      'close_style_message', 'close_style_detail', 'close_style_action', 'toast_config_corrupt_backup']) {
      assert(t(key) && t(key) !== key, `${key} is localized (${lang})`);
    }
  }
  assert(t('toast_config_corrupt_backup').includes('{name}'), 'Corrupt config toast names the backup file');
  assert(/invoke\('take_startup_warning'\)/.test(appSource) && /toast_config_corrupt_backup/.test(appSource),
    'Startup shows a one-time warning when config.json was moved aside');
  setLanguage('es');

  // Visual and i18n defects from the 2026-10-14 screenshot
  assert(/data-i18n="app_version">v[\d.]+<\/span>/.test(htmlContent) && !/Tauri 2/.test(htmlContent),
    'Header badge shows only the app version, without the internal framework label');
  assert(!/app_version: '[^']*Tauri/.test(i18nContent),
    'Localized app_version tables drop the internal framework label');
  assert(appSource.includes('loading: t(\'status_loading\')'),
    'Status pill maps the loading state to a translated label');
  setLanguage('es');
  assert(t('status_loading') === 'Cargando...', 'Spanish loading status translation exists');
  assert(t('runtime_setup_worker_starting') === 'Iniciando el motor ASR y esperando a que esté listo…',
    'Spanish ASR worker startup line is localized');
  setLanguage('en');
  assert(t('status_loading') === 'Loading...', 'English loading status translation exists');
  assert(t('runtime_setup_worker_starting') === 'Starting the ASR worker and waiting for it to become ready…',
    'English ASR worker startup line is localized');
  setLanguage('es');
  assert(appSource.includes("phase === 'starting_worker'") && appSource.includes("t('runtime_setup_worker_starting')"),
    'Startup progress line uses the localized text for the starting_worker phase');
  assert(!/\.feed-item\.feed-item-blocked\s*\{[^}]*opacity/.test(cssContent)
    && /\.feed-item\.feed-item-blocked\s*>\s*\.feed-item-body\s*\{[^}]*opacity/.test(cssContent),
    'Blocked feed items dim only their text, so the kebab menu opacity matches other items');
  assert(/#runtime-setup-progress\[hidden\]\s*\{\s*display:\s*none/.test(cssContent)
    && /#runtime-setup-meter\s*\{[^}]*width:\s*100%/.test(cssContent),
    'Startup progress block is styled as hint text with a full-width meter, hidden when idle');
  assert(appSource.includes('elements.inputObsUrl.title = obs.url'),
    'Truncated OBS URL field exposes the full URL as a tooltip');

  // Verify robust state synchronization, serialized queue, keyboard navigation and bounds clamping
  assert(appSource.includes('adoptSavedConfig(newConfig, saved);'),
    'savedConfig baseline is synchronized from the backend-returned config on blacklist and prompt additions');
  assert(appSource.includes('queueConfigSave'),
    'Config saves are serialized through queueConfigSave to prevent race conditions');
  assert(appSource.includes('markFeedItemsBlocked'),
    'markFeedItemsBlocked marks all occurrences of blacklisted phrases in the feed');
  assert(appSource.includes("e.key === 'ArrowDown'") && appSource.includes("e.key === 'ArrowUp'"),
    'Feed action dropdown supports ARIA keyboard arrow navigation');
  assert(appSource.includes('viewportWidth') && appSource.includes('viewportHeight'),
    'Dropdown menu position is bounded within window viewport boundaries');
  assert(appSource.includes('copyTextFallback'),
    'Clipboard copying provides resilient execCommand fallback');
  assert(appSource.includes('prunedHeight'),
    'Circular buffer pruning compensates scroll offset when user is reading history');

  // Guard: the Rust restart-required list must mirror the desktop runtime list (minus the two keys
  // that never need a service restart: the device selection and the output folder)
  const runtimeSettingsSource = fs.readFileSync(path.join(__dirname, 'src', 'runtime-settings.js'), 'utf8');
  const rustPatchSource = fs.readFileSync(path.join(__dirname, '..', 'crates', 'liveaudio-core', 'src', 'config', 'patch.rs'), 'utf8');
  const restartKeysBody = rustPatchSource.match(/RESTART_REQUIRED_KEYS: &\[&str\] = &\[([\s\S]*?)\];/);
  const runtimeKeysBody = runtimeSettingsSource.match(/const RUNTIME_SETTINGS = \[([\s\S]*?)\];/);
  const restartRequiredKeys = new Set([...(restartKeysBody?.[1] || '').matchAll(/"([a-z_]+)"/g)].map((m) => m[1]));
  const runtimeKeys = new Set([...(runtimeKeysBody?.[1] || '').matchAll(/'([a-z_]+)'/g)].map((m) => m[1]));
  for (const key of ['audio_device', 'output_dir']) runtimeKeys.delete(key);
  const onlyInRust = [...restartRequiredKeys].filter((k) => !runtimeKeys.has(k));
  const onlyInRuntime = [...runtimeKeys].filter((k) => !restartRequiredKeys.has(k));
  assert(restartKeysBody && runtimeKeysBody && onlyInRust.length === 0 && onlyInRuntime.length === 0,
    `RESTART_REQUIRED_KEYS mirrors RUNTIME_SETTINGS (only in Rust: [${onlyInRust}], only in JS: [${onlyInRuntime}])`);

  // The Save button goes through the same queue as style saves, so the two never interleave
  assert(/saveGeneralConfig = \(\) => queueConfigSave\(/.test(appJsContent) && /btnSaveConfig\.addEventListener\('click', \(\) => saveGeneralConfig\(\)\)/.test(appJsContent),
    'Save button routes its save through queueConfigSave');

  // Style saves adopt the config save_subtitle_style returns, so a later style save cannot revert server-side changes
  const styleSaveBody = (appJsContent.match(/async function saveSubtitleStyle\(\) \{([\s\S]*?)\n\}/) || [])[1] || '';
  assert(/const updated = await invoke\('save_subtitle_style'/.test(styleSaveBody)
    && /adoptSavedStyle\(/.test(styleSaveBody)
    && !/savedConfig = \{ \.\.\.savedConfig, subtitle_theme/.test(styleSaveBody),
    'Style save adopts the config returned by save_subtitle_style, not a copy of the pre-save payload');
  // An external theme change refreshes the Style tab even when the general form has unsaved edits
  const externalHandler = (appJsContent.match(/function handleExternalConfigChange\(([\s\S]*?)\n\}/) || [])[0] || '';
  assert(/changed_keys/.test(externalHandler)
    && /refreshStyleTab\(config\)/.test(externalHandler)
    && /async function refreshStyleTab|function refreshStyleTab\(/.test(appJsContent),
    'External subtitle theme changes refresh the Style tab even when the form is dirty');

  // Save handlers adopt the config the backend returns (it may hold external edits), never the form values
  const adoptingBlacklistOrPromptSaves = appJsContent.match(
    /const saved = await invoke\('save_config', \{ config: newConfig, base: savedConfig \}\);\s*adoptSavedConfig\(newConfig, saved\);/g
  ) || [];
  assert(adoptingBlacklistOrPromptSaves.length >= 2,
    'Blacklist and prompt saves adopt the config the backend returned, sent config as the reference');
  assert(!/populateForm\(saved\)|populateForm\(backendConfig\)/.test(appJsContent),
    'Save handlers never overwrite the whole form with the backend config (would drop edits made during the save)');
  assert(!/savedConfig = structuredClone\(newConfig\)/.test(appJsContent)
    && !/savedConfig = structuredClone\(updatedConfig\)/.test(appJsContent),
    'No save handler takes the saved baseline from the form instead of the backend');
  assert(/backendConfig = await invoke\('save_config', \{ config, base: savedConfig \}\)/.test(appJsContent)
    && /adoptSavedConfig\(updatedConfig, backendConfig\)/.test(appJsContent),
    'Runtime settings save adopts the backend config, sent config as the reference');
  // Behavior: fields the user edited while the save was in flight keep the user's value; untouched fields take the backend value
  const mergeSavedConfigIntoForm = runtimeSettings.mergeSavedConfigIntoForm;
  assert(typeof mergeSavedConfigIntoForm === 'function', 'mergeSavedConfigIntoForm helper is exported');
  if (typeof mergeSavedConfigIntoForm === 'function') {
    const sent = { ws_port: 9000, blacklist: 'a', language: 'es' };
    const typedDuringSave = { ws_port: 9000, blacklist: 'typed while saving', language: 'es' };
    const backend = { ws_port: 9100, blacklist: 'a', language: 'es' };
    const merged = mergeSavedConfigIntoForm(sent, typedDuringSave, backend);
    assert(merged.blacklist === 'typed while saving' && merged.ws_port === 9100,
      'In-flight edits survive a save and untouched fields adopt the backend value');
    const unchanged = mergeSavedConfigIntoForm(sent, { ...sent }, backend);
    assert(JSON.stringify(unchanged) === JSON.stringify(backend),
      'With no in-flight edits the form shows exactly the backend config');
  }

  // Verify accessible HTML attributes and translations
  assert(htmlContent.includes('data-i18n-aria-label="aria_subtitles_feed"'),
    'Feed container has localized aria-label attribute');
  assert(htmlContent.includes('data-i18n-title="title_feed_count"'),
    'Feed counter badge has localized title attribute');
  assert(htmlContent.includes('data-i18n-title="title_test_subtitle"'),
    'Test subtitle button has localized title attribute');
  assert(htmlContent.includes('data-i18n-title="title_clear_feed"'),
    'Clear feed button has localized title attribute');
  setLanguage('es');
  assert(t('aria_subtitles_feed') === 'Feed de subtítulos en vivo',
    'Spanish feed accessible name translation exists');
  assert(t('title_feed_count') === 'Subtítulos en búfer',
    'Spanish feed count title translation exists');
  setLanguage('en');
  assert(t('aria_subtitles_feed') === 'Live subtitles feed',
    'English feed accessible name translation exists');
  assert(t('title_feed_count') === 'Subtitles in buffer',
    'English feed count title translation exists');

  // 9. OBS overlay theme engine (liveaudio/assets/subtitulos_obs.html): validation and apply
  const overlayPath = fs.existsSync(path.join(__dirname, '..', 'assets', 'subtitulos_obs.html')) ? path.join(__dirname, '..', 'assets', 'subtitulos_obs.html') : path.join(__dirname, '..', 'liveaudio', 'assets', 'subtitulos_obs.html');
  const overlaySource = fs.readFileSync(overlayPath, 'utf8');
  const themeBlock = overlaySource.match(/\/\* THEME-ENGINE:BEGIN \*\/([\s\S]*?)\/\* THEME-ENGINE:END \*\//)?.[1] ?? '';
  const overlayProps = new Map();
  const overlayDataset = {};
  const fakeDocument = {
    documentElement: {
      style: {
        setProperty: (key, value) => overlayProps.set(key, value),
        removeProperty: (key) => overlayProps.delete(key),
      },
      dataset: overlayDataset,
    },
  };
  let overlayEngine = null;
  try {
    overlayEngine = new Function('document', `${themeBlock}
return { sanitizeThemeTokens, applyTheme };`)(fakeDocument);
  } catch (error) {
    console.error(`Overlay theme engine failed to load: ${error.message}`);
  }
  const sanitizeOverlay = overlayEngine?.sanitizeThemeTokens ?? (() => ({}));
  assert(typeof overlayEngine?.sanitizeThemeTokens === 'function',
    'Overlay theme engine is delimited by THEME-ENGINE markers and exposes sanitizeThemeTokens');
  assert(sanitizeOverlay({ '--sub-color': '#ff0000' })['--sub-color'] === '#ff0000',
    'Overlay keeps a valid hex color');
  assert(sanitizeOverlay({ '--sub-bg': 'rgba(0, 0, 0, 0.7)' })['--sub-bg'] === 'rgba(0, 0, 0, 0.7)',
    'Overlay keeps a valid rgba color');
  assert(!('--sub-color' in sanitizeOverlay({ '--sub-color': 'rgb(1, 2)' })),
    'Overlay rejects an rgb color with too few channels');
  assert(!('--sub-bg' in sanitizeOverlay({ '--sub-bg': 'url(http://evil.test/x.png)' })),
    'Overlay rejects url() inside a color token');
  assert(!('--sub-font-family' in sanitizeOverlay({ '--sub-font-family': 'Arial; } body { display: none' })),
    'Overlay rejects a font family with block characters');
  assert(!('--sub-font-size' in sanitizeOverlay({ '--sub-font-size': '500px' })),
    'Overlay ignores an out-of-range font size');
  assert(sanitizeOverlay({ '--sub-font-size': '48px' })['--sub-font-size'] === '48px',
    'Overlay keeps an in-range font size');
  assert(!('--evil' in sanitizeOverlay({ '--evil': '1' })), 'Overlay drops unknown tokens');
  assert(sanitizeOverlay({ '--sub-animation': 'fade' })['--sub-animation'] === 'fade',
    'Overlay accepts a fixed-enum animation');
  assert(!('--sub-animation' in sanitizeOverlay({ '--sub-animation': 'spin' })),
    'Overlay rejects an animation outside the enum');
  assert(sanitizeOverlay({ '--sub-padding': '10px 20px' })['--sub-padding'] === '10px 20px',
    'Overlay accepts a padding list');
  assert(!('--sub-padding' in sanitizeOverlay({ '--sub-padding': '10px; color: red' })),
    'Overlay rejects padding with CSS injection');
  const applyOverlay = overlayEngine?.applyTheme ?? (() => {});
  applyOverlay({ '--sub-color': '#00ff00', '--sub-bg': 'url(x)' });
  assert(overlayProps.get('--sub-color') === '#00ff00' && !overlayProps.has('--sub-bg'),
    'applyTheme sets valid tokens and ignores invalid ones');
  applyOverlay({});
  assert(!overlayProps.has('--sub-color'),
    'applyTheme resets tokens that are missing from the newest theme');
  applyOverlay({ '--sub-animation': 'none' });
  assert(overlayDataset.anim === 'none', 'applyTheme exposes the animation mode for CSS');
  applyOverlay({ '--sub-color': '#00ff00', '--sub-bg': 'rgba(1, 2, 3, 0.5)' });
  assert(overlayProps.get('--sub-user-color') === '#00ff00' && overlayProps.get('--sub-user-bg') === 'rgba(1, 2, 3, 0.5)',
    'applyTheme exposes user-set color and background so every preset can honor them');
  applyOverlay({});
  assert(!overlayProps.has('--sub-user-color') && !overlayProps.has('--sub-user-bg'),
    'applyTheme clears user overrides when the theme no longer sets them');
  const parityCases = JSON.parse(fs.readFileSync(path.join(__dirname, '..', 'crates', 'liveaudio-core', 'tests', 'fixtures', 'subtitle_theme_cases.json'), 'utf8'));
  const rejectedValid = parityCases.valid.filter(([key, value]) => sanitizeOverlay({ [key]: value })[key] !== value);
  assert(rejectedValid.length === 0, `Overlay accepts every shared valid case (rejected: ${JSON.stringify(rejectedValid)})`);
  const acceptedInvalid = parityCases.invalid.filter(([key, value]) => key in sanitizeOverlay({ [key]: value }));
  assert(acceptedInvalid.length === 0, `Overlay rejects every shared invalid case (accepted: ${JSON.stringify(acceptedInvalid)})`);
  const presetRule = (name) => overlaySource.match(new RegExp(String.raw`\.style-${name}\s*\{([\s\S]*?)\}`))?.[1] ?? '';
  assert(presetRule('karaoke').includes('var(--sub-user-color, #ffcc00)'),
    'Karaoke text honors a user-set color and keeps its yellow default');
  for (const name of ['neon', 'minimal', 'bold', 'typewriter']) {
    assert(presetRule(name).includes('var(--sub-user-bg,'), `${name} background honors a user-set background`);
  }

  // 10. Subtitle style helpers (desktop/src/subtitle-style.js): font list
  const subtitleStyle = await import('./src/subtitle-style.js').catch((error) => {
    console.error(`subtitle-style.js failed to load: ${error.message}`);
    return {};
  });
  const fontFamilies = subtitleStyle.SUBTITLE_FONT_FAMILIES ?? [];
  assert(Array.isArray(fontFamilies) && fontFamilies.length >= 5,
    'Subtitle font list is a fallback array with common fonts');
  assert(fontFamilies.includes('Segoe UI') && fontFamilies.includes('Arial'),
    'Subtitle font list includes Segoe UI and Arial');
  assert(new Set(fontFamilies).size === fontFamilies.length,
    'Subtitle font list has no duplicates');
  assert(fontFamilies.every((name) => /^[A-Za-z0-9 -]+$/.test(name) && name.length <= 200),
    'Every subtitle font name uses the safe font charset');
  const windowsFonts = subtitleStyle.fontFamiliesForPlatform?.('Win32') ?? [];
  const linuxFonts = subtitleStyle.fontFamiliesForPlatform?.('Linux x86_64') ?? [];
  assert(windowsFonts.includes('Segoe UI') && !windowsFonts.includes('DejaVu Sans'),
    'The Windows font list offers Windows families only');
  assert(linuxFonts.includes('DejaVu Sans') && !linuxFonts.includes('Segoe UI'),
    'The Linux font list offers Linux families only');
  assert((subtitleStyle.fontFamiliesForPlatform?.('') ?? []).length === fontFamilies.length,
    'An unknown platform keeps the full font list');

  // 11. Style tab helpers: controls <-> theme tokens, and named presets
  const styleHelpers = { ...subtitleStyle };
  const { themeFromControls, controlsFromTheme, savePreset, deletePreset, SUBTITLE_STYLE_DEFAULTS } = styleHelpers;
  const defaultControls = { ...(SUBTITLE_STYLE_DEFAULTS ?? {}) };
  assert(typeof themeFromControls === 'function' && typeof controlsFromTheme === 'function',
    'Style helpers export themeFromControls and controlsFromTheme');
  assert(JSON.stringify(themeFromControls(defaultControls)) === '{}',
    'Default controls produce an empty theme, so the overlay keeps its own defaults');
  const sampleTheme = themeFromControls({ ...defaultControls, fontSize: 60, color: '#FFCC00', animation: 'fade', animationDuration: 0.5, fontFamily: 'Arial' });
  assert(sampleTheme['--sub-font-size'] === '60px' && sampleTheme['--sub-color'] === '#ffcc00'
    && sampleTheme['--sub-animation'] === 'fade' && sampleTheme['--sub-animation-duration'] === '0.5s'
    && sampleTheme['--sub-font-family'] === 'Arial',
    'Changed controls map to CSS tokens with units, lowercase hex and the font list');
  assert(!('--sub-bg' in sampleTheme) && !('--sub-font-weight' in sampleTheme),
    'Untouched controls are not written to the theme');
  const clampedTheme = themeFromControls({ ...defaultControls, fontSize: 500, animationDuration: 9, fontFamily: 'url(x)', color: 'red;' });
  assert(clampedTheme['--sub-font-size'] === '120px' && clampedTheme['--sub-animation-duration'] === '2s',
    'Out-of-range numeric controls are clamped to the Rust ranges');
  assert(!('--sub-font-family' in clampedTheme) && !('--sub-color' in clampedTheme),
    'Font and color controls outside their allowed sets are not emitted');
  const roundTrip = controlsFromTheme(sampleTheme);
  assert(roundTrip.fontSize === 60 && roundTrip.color === '#ffcc00' && roundTrip.animation === 'fade'
    && roundTrip.animationDuration === 0.5 && roundTrip.fontFamily === 'Arial',
    'controlsFromTheme reads the same values back');
  assert(controlsFromTheme({}).fontSize === defaultControls.fontSize,
    'controlsFromTheme falls back to defaults for missing tokens');
  const opaqueBlack = themeFromControls({ ...defaultControls, background: '#000000', backgroundOpacity: 0.7 });
  assert(opaqueBlack['--sub-bg'] === 'rgba(0, 0, 0, 0.7)',
    'A user-set background is always written, even black, with its opacity');
  const tinted = themeFromControls({ ...defaultControls, background: '#102030', backgroundOpacity: 0.5 });
  assert(tinted['--sub-bg'] === 'rgba(16, 32, 48, 0.5)',
    'A user-set background is written as rgba with the chosen opacity');
  assert(!('--sub-bg' in themeFromControls({ ...defaultControls, background: null })),
    'No user background leaves the overlay default in place');
  const readBg = controlsFromTheme({ '--sub-bg': 'rgba(16, 32, 48, 0.5)' });
  assert(readBg.background === '#102030' && readBg.backgroundOpacity === 0.5,
    'controlsFromTheme reads rgba back into hex plus opacity');
  const noBg = controlsFromTheme({});
  assert(noBg.background === null && noBg.backgroundOpacity === defaultControls.backgroundOpacity,
    'controlsFromTheme reports no user background when the token is missing');
  const preserved = themeFromControls({ ...defaultControls, fontSize: 60 }, { '--sub-radius': '8px', '--sub-shadow': '2px 2px 0 #000', '--sub-font-size': '48px' });
  assert(preserved['--sub-radius'] === '8px' && preserved['--sub-shadow'] === '2px 2px 0 #000',
    'Tokens the controls do not edit are kept');
  assert(preserved['--sub-font-size'] === '60px', 'Tokens the controls edit are replaced, not kept');

  const loadPreset = typeof styleHelpers.loadPresetTheme === 'function' ? styleHelpers.loadPresetTheme : () => ({ theme: {}, controls: {} });
  const neonTokens = { '--sub-color': '#00ffaa', '--sub-radius': '12px', '--sub-shadow': '0 2px 4px #000' };
  const loadedNeon = loadPreset({ name: 'Neon', tokens: { ...neonTokens } });
  assert(JSON.stringify(loadedNeon.theme) === JSON.stringify(neonTokens),
    'Loading a preset makes its full token set the theme, including tokens the controls do not edit');
  assert(loadedNeon.controls.color === '#00ffaa' && loadedNeon.controls.fontSize === defaultControls.fontSize,
    'Loading a preset returns controls for its values and defaults for missing ones');
  const afterLoadSave = themeFromControls(loadedNeon.controls, loadedNeon.theme);
  assert(afterLoadSave['--sub-radius'] === '12px' && afterLoadSave['--sub-shadow'] === '0 2px 4px #000',
    'Saving right after a load keeps the loaded preset extras');
  assert(!('--sub-border' in afterLoadSave),
    'Saving right after a load does not leak tokens from the previously active theme');

  let savedPresets = [];
  const first = savePreset(savedPresets, '  Night  ', { '--sub-color': '#112233' });
  assert(!first.error && first.presets.length === 1 && first.presets[0].name === 'Night',
    'savePreset trims the name and stores the preset');
  const replaced = savePreset(first.presets, 'Night', { '--sub-color': '#445566' });
  assert(!replaced.error && replaced.presets.length === 1 && replaced.presets[0].tokens['--sub-color'] === '#445566',
    'Saving under an existing name replaces that preset');
  assert(savePreset(first.presets, '   ', {}).error === 'name', 'An empty preset name is rejected');
  assert(savePreset(first.presets, 'x'.repeat(41), {}).error === 'name', 'A preset name over 40 characters is rejected');
  const full = Array.from({ length: 20 }, (_, i) => ({ name: `p${i}`, tokens: {} }));
  assert(savePreset(full, 'extra', {}).error === 'limit', 'The 21st preset is rejected');
  const removed = deletePreset(replaced.presets, 'Night');
  assert(removed.length === 0, 'deletePreset removes the named preset');
  assert(deletePreset(replaced.presets, 'Missing').length === 1, 'deletePreset ignores unknown names');
  savedPresets = [];

  // U2: installed fonts, preset base style, time on screen, preview timeline
  const { fontChoices, presetOptionValue, presetNameFromOption, previewWordDelays, previewHoldMs } = styleHelpers;
  assert(typeof fontChoices === 'function' && JSON.stringify(fontChoices(['Bahnschrift', 'Arial'], ['Segoe UI'], 'Arial')) === JSON.stringify(['Bahnschrift', 'Arial']),
    'The font list is the installed families when there are any');
  assert(JSON.stringify(fontChoices([], ['Segoe UI', 'Arial'], 'Segoe UI')) === JSON.stringify(['Segoe UI', 'Arial']),
    'With no installed list the curated platform fonts are the fallback');
  assert(JSON.stringify(fontChoices(['Arial'], ['Segoe UI'], 'Gone Font')) === JSON.stringify(['Arial', 'Gone Font']),
    'A saved font outside the list stays selectable');
  const installedFont = themeFromControls({ ...defaultControls, fontFamily: 'Bahnschrift' });
  assert(installedFont['--sub-font-family'] === 'Bahnschrift' && controlsFromTheme(installedFont).fontFamily === 'Bahnschrift',
    'An installed font round-trips through the theme');
  assert(!('--sub-font-family' in themeFromControls({ ...defaultControls, fontFamily: "Evil'; } x{" })),
    'A font name outside the safe charset is not emitted');
  assert(SUBTITLE_STYLE_DEFAULTS.lifetime === 5 && !('--sub-lifetime' in themeFromControls(defaultControls)),
    'Time on screen defaults to 5 s and is not written at its default');
  assert(themeFromControls({ ...defaultControls, lifetime: 8 })['--sub-lifetime'] === '8s'
    && themeFromControls({ ...defaultControls, lifetime: 99 })['--sub-lifetime'] === '15s'
    && themeFromControls({ ...defaultControls, lifetime: 0.2 })['--sub-lifetime'] === '1.5s',
  'Time on screen is written in seconds and clamped to 1.5-15 s');
  assert(controlsFromTheme({ '--sub-lifetime': '8s' }).lifetime === 8 && controlsFromTheme({}).lifetime === 5,
    'Time on screen reads back from the theme');
  assert(themeFromControls({ ...defaultControls }, { '--sub-lifetime': '9s' })['--sub-lifetime'] === undefined,
    'The Style tab owns --sub-lifetime');
  const withStyle = savePreset([], 'Night', { '--sub-color': '#112233' }, 'neon');
  assert(withStyle.presets[0].style === 'neon' && !('style' in savePreset([], 'Night', {}).presets[0]),
    'savePreset records the base style when given');
  assert(loadPreset({ name: 'N', style: 'bold', tokens: {} }).style === 'bold' && loadPreset({ name: 'N', tokens: {} }).style === null,
    'Loading a preset returns its base style, or null for old presets');
  assert(presetNameFromOption(presetOptionValue('Night')) === 'Night' && presetNameFromOption('neon') === null,
    'Preset selector options are told apart from built-in styles');
  assert(JSON.stringify(previewWordDelays('karaoke', 5)) === JSON.stringify([0, 25, 50, 75, 100]) && previewWordDelays('default', 5).length === 0
    && previewWordDelays('typewriter', 100)[1] === 400 / 99,
  'Word reveal delays follow the overlay stagger and its 400 ms budget');
  for (const [style, max] of [['karaoke', 25], ['rgb', 50], ['typewriter', 40]]) {
    assert(overlaySource.includes(`Math.min(${max}, REVEAL_BUDGET_MS`), `The preview stagger for ${style} matches the overlay (${max} ms)`);
  }
  assert(previewHoldMs(5) === 3000 && previewHoldMs(1.5) === 1500,
    'The preview holds the line for the time on screen, capped at 3 s');

  // 12. Style tab markup and localization (index.html, i18n.js)
  const styleSection = htmlContent.match(/<section id="tab-style"[\s\S]*?<\/section>/)?.[0] || '';
  assert(htmlContent.includes('id="tab-style-tab"') && htmlContent.includes('data-tab="tab-style"'),
    'Style tab button exists and points at the Style panel');
  assert(styleSection.length > 0 && styleSection.includes('role="tabpanel"'),
    'Style tab panel exists as a tabpanel');
  const styleControlIds = [
    'select-sub-font', 'input-sub-font-size', 'input-sub-letter-spacing', 'input-sub-word-spacing',
    'select-sub-weight', 'select-sub-animation',
    'input-sub-color', 'input-sub-bg', 'check-sub-bg-custom', 'input-sub-bg-opacity', 'input-sub-anim-duration', 'sub-style-preview',
    'optgroup-sub-presets', 'input-sub-preset-name', 'btn-sub-preset-save',
    'btn-sub-preset-delete', 'sub-style-status', 'input-sub-lifetime', 'btn-sub-preview-play',
  ];
  for (const id of styleControlIds) {
    assert(styleSection.includes(`id="${id}"`), `Style tab contains #${id}`);
  }
  const styleLabelKeys = [
    'tab_style', 'lbl_sub_font', 'lbl_sub_font_size', 'lbl_sub_letter_spacing', 'lbl_sub_word_spacing',
    'lbl_sub_weight', 'opt_weight_400', 'opt_weight_700',
    'opt_weight_900', 'lbl_sub_animation', 'opt_anim_pop', 'opt_anim_fade', 'opt_anim_none', 'lbl_sub_color',
    'lbl_sub_bg', 'lbl_sub_bg_custom', 'lbl_sub_bg_opacity', 'lbl_sub_anim_duration', 'sub_style_preview_text', 'hint_sub_style_live', 'lbl_sub_my_styles', 'lbl_sub_lifetime', 'title_sub_preview_play',
    'lbl_sub_preset_name', 'btn_sub_preset_save', 'btn_sub_preset_delete',
    'sub_preset_saved', 'sub_preset_deleted', 'sub_preset_loaded', 'sub_preset_err_name',
    'sub_preset_err_limit', 'sub_style_save_failed', 'lbl_sub_builtin_style', 'hint_sub_builtin_style',
  ];
  const i18nModule = await import('./src/i18n.js');
  const missingKeys = styleLabelKeys.filter((key) => !i18nModule.TRANSLATIONS.es?.[key] || !i18nModule.TRANSLATIONS.en?.[key]);
  assert(missingKeys.length === 0, `Style strings exist in both es and en (missing: ${missingKeys.join(', ') || 'none'})`);
  const styleAppSource = fs.readFileSync(path.join(__dirname, 'src', 'app.js'), 'utf8');
  assert(styleAppSource.includes('queueConfigSave(saveSubtitleStyle)'),
    'Style saves go through the config save queue, so they never interleave with general saves');
  assert(styleAppSource.includes('loadPresetTheme(preset)') && styleAppSource.includes('subtitle_theme: { ...loaded.theme }'),
    'Choosing a saved style makes the loaded theme the current subtitle theme before saving');
  assert(styleAppSource.includes("invoke('list_system_fonts')"), 'The Style tab asks the backend for the installed fonts');
  assert(!styleSection.includes('select-sub-preset') && !styleSection.includes('btn-sub-preset-load') && styleSection.includes('<optgroup id="optgroup-sub-presets"'),
    'Saved styles live in the style selector, not in a second dropdown');

  // 12. Style tab ownership (F1): a style save touches only the style fields; the general form never merges them
  const { adoptSavedStyle, STYLE_CONFIG_KEYS } = runtimeSettings;
  const oldTheme = { '--sub-font-size': '42px' };
  const newTheme = { '--sub-font-size': '60px', '--sub-color': '#ffcc00' };
  const staleConfig = { subtitle_theme: oldTheme, subtitle_presets: [], blacklist: 'typed earlier', ws_port: 8765 };
  assert(Array.isArray(STYLE_CONFIG_KEYS) && STYLE_CONFIG_KEYS.includes('subtitle_theme') && STYLE_CONFIG_KEYS.includes('subtitle_presets'),
    'The style fields (subtitle_theme, subtitle_presets) are declared in one shared list');
  const afterStyleSave = typeof adoptSavedStyle === 'function'
    ? adoptSavedStyle(staleConfig, { ...staleConfig, subtitle_theme: newTheme, blacklist: 'disk value' })
    : {};
  assert(JSON.stringify(afterStyleSave.subtitle_theme) === JSON.stringify(newTheme),
    'Style save adoption takes the saved theme even when the client copy was stale (snap-back regression)');
  assert(afterStyleSave.blacklist === 'typed earlier' && afterStyleSave.ws_port === 8765,
    'Style save adoption leaves general fields as the user left them');
  const generalMerged = runtimeSettings.mergeSavedConfigIntoForm(
    { ...staleConfig, ws_port: 9000 },
    { ...staleConfig, ws_port: 9000 },
    { ...staleConfig, subtitle_theme: newTheme, ws_port: 9100 },
  );
  assert(JSON.stringify(generalMerged.subtitle_theme) === JSON.stringify(oldTheme) && generalMerged.ws_port === 9100,
    'General save merge never takes style fields from the backend; other fields still adopt the backend value');
  // Edit typed while a style save is in flight: the saved theme becomes the config (the next debounced save
  // sends the typed value); the controls are re-rendered only when no edit happened since the send
  const inFlight = adoptSavedStyle(staleConfig, { ...staleConfig, subtitle_theme: newTheme });
  assert(JSON.stringify(inFlight.subtitle_theme) === JSON.stringify(newTheme) && inFlight.blacklist === 'typed earlier',
    'Adopting a style save while the user types keeps the general form intact');

  const populateFormBody = (styleAppSource.match(/function populateForm\(config[\s\S]*?\n\}/) || [])[0] || '';
  assert(populateFormBody.length > 0 && !/populateStyleControls\(/.test(populateFormBody),
    'populateForm never repopulates style controls, so general saves cannot revert Style tab edits');
  const styleSaveBodyNow = (styleAppSource.match(/async function saveSubtitleStyle\(\) \{([\s\S]*?)\n\}/) || [])[1] || '';
  assert(/adoptSavedStyle\(/.test(styleSaveBodyNow) && !/adoptSavedConfig\(/.test(styleSaveBodyNow),
    'Style save adopts only the style fields returned by save_subtitle_style');
  const renderStyleTabBody = (styleAppSource.match(/function renderStyleTab\([\s\S]*?\n\}/) || [])[0] || '';
  assert(/renderStyleTab\(updated\)/.test(styleSaveBodyNow) && /if \(!force && styleDirty\) return;/.test(renderStyleTabBody),
    'Style controls re-render from the saved theme only when no edit happened since the send');

  // 13. Color pickers show a visible swatch (F2). Structural check: the text-input padding must not
  // shrink the picker, so a scoped rule gives the color input its own box and swatch.
  const colorRule = (cssContent.match(/input\.form-control\[type="color"\]\s*\{([^}]*)\}/) || [])[1] || '';
  assert(/height:\s*\d+px/.test(colorRule) && /padding:\s*\d+px/.test(colorRule),
    'Color pickers have a scoped rule with a fixed height and padding, so the swatch is visible');
  assert(/::-webkit-color-swatch\s*\{/.test(cssContent),
    'Color picker swatch is styled inside the picker, so it is not hidden by the text-field padding');

  // 14. Built-in subtitle style lives in the Style tab (F3): one select, saved through the style path
  const styleTabStart = htmlContent.indexOf('id="tab-style"');
  const selectStyleAt = htmlContent.indexOf('id="select-subtitle-style"');
  assert(styleTabStart !== -1 && selectStyleAt > styleTabStart
    && !htmlContent.slice(styleTabStart, selectStyleAt).includes('<section')
    && htmlContent.indexOf('id="select-subtitle-style"', selectStyleAt + 1) === -1,
    'The built-in style select is in the Style tab and appears exactly once');
  const rustStylesBody = (fs.readFileSync(path.join(__dirname, '..', 'crates', 'liveaudio-core', 'src', 'config', 'mod.rs'), 'utf8')
    .match(/VALID_SUBTITLE_STYLES: &\[&str\] = &\[([\s\S]*?)\];/) || [])[1] || '';
  const rustStyles = [...rustStylesBody.matchAll(/"([a-z]+)"/g)].map((m) => m[1]);
  const selectStyleBody = (htmlContent.match(/<select id="select-subtitle-style"[\s\S]*?<\/select>/) || [])[0] || '';
  const selectStyleValues = [...selectStyleBody.matchAll(/<option value="([a-z]+)"/g)].map((m) => m[1]);
  assert(rustStyles.length === 7 && JSON.stringify(selectStyleValues) === JSON.stringify(rustStyles),
    'The built-in style options match VALID_SUBTITLE_STYLES in the Rust config, in order');
  assert(runtimeSettings.STYLE_CONFIG_KEYS.includes('subtitle_style'),
    'subtitle_style is owned by the style path, so the general form never merges it');
  const styleOwnedMerged = runtimeSettings.mergeSavedConfigIntoForm(
    { subtitle_style: 'default', ws_port: 9000 },
    { subtitle_style: 'neon', ws_port: 9000 },
    { subtitle_style: 'karaoke', ws_port: 9100 },
  );
  assert(styleOwnedMerged.subtitle_style === 'neon' && styleOwnedMerged.ws_port === 9100,
    'A general save keeps the form copy of subtitle_style and adopts the other backend fields');
  assert(runtimeSettings.adoptSavedStyle({ subtitle_style: 'default', blacklist: 'x' }, { subtitle_style: 'karaoke' }).subtitle_style === 'karaoke',
    'A style save adopts the backend subtitle_style');
  const styleAppBody = (styleAppSource.match(/async function saveSubtitleStyle\(\) \{([\s\S]*?)\n\}/) || [])[1] || '';
  assert(/invoke\('save_subtitle_style', \{ theme, presets, subtitleStyle/.test(styleAppBody),
    'Style save sends the built-in style with the theme and presets');
  assert(/selectSubtitleStyle\.addEventListener\('change'[\s\S]{0,200}scheduleSubtitleStyleSave\(0\)/.test(styleAppSource),
    'Choosing a built-in style saves it immediately through the style save path');
  assert(!/cfg\.subtitle_style = elements\.selectSubtitleStyle/.test(styleAppSource),
    'The general form does not read the built-in style select');
  assert(/subStylePreviewText\.className = `sub-box visible style-\$\{/.test(styleAppSource)
    && /id="sub-style-preview-text"/.test(htmlContent),
    'The Style tab preview shows the chosen built-in style');

  // 15. Style tab through the real app.js (vm harness): save -> adopt, edits during a save, external changes
  // The stub has no <option> elements, so the model select keeps ''; the blacklists and flags are set so the form round-trips.
  const styleBaseConfig = {
    language: 'es', asr_language: 'es', model_size: '', device: 'cpu', cpu_threads: 4, silence_timeout: 1.5,
    max_chunk_duration: 10, ws_port: 8765, blacklist: 'x', whisper_context_prompt_es: '', whisper_context_prompt_en: '',
    extra: { blacklist_es: 'x', blacklist_en: 'x' }, continuous_session: false, save_transcript_enabled: false,
    save_vtt_enabled: false, rnnoise_enabled: false, mcp_style_enabled: false, mcp_system_enabled: false, acoustic_events_enabled: false, live_partial_subtitles_enabled: false, asr_backend: 'auto',
    selected_profile_id: 'balanced', profile_mode: 'preset',
    subtitle_style: 'default', subtitle_theme: {}, subtitle_presets: [], subtitle_display_mode: 'adaptive',
    subtitle_ribbon_max_lines: 3, subtitle_backlog_policy: 'auto', subtitle_max_live_delay_sec: 10,
    subtitle_catchup_interval_sec: 2, vad_speech_pad_ms: 200, vad_threshold: 0.5,
    check_updates_enabled: true,
  };
  {
    const h = await loadAppHarness(styleBaseConfig);
    h.el('input-sub-color').value = '#ff0000';
    h.el('input-sub-color').dispatch('input');
    await h.runTimers();
    assert(h.styleSaves().length === 1 && h.backend.config.subtitle_theme['--sub-color'] === '#ff0000',
      'A Style control change is saved after the debounce');
    assert(h.el('input-sub-color').value === '#ff0000' && h.app.currentConfig.subtitle_theme['--sub-color'] === '#ff0000',
      'Adopting the saved style keeps the new value on screen and in the config');
  }
  {
    const h = await loadAppHarness(styleBaseConfig);
    h.backend.holdStyleSaves = true;
    h.el('input-sub-color').value = '#ff0000';
    h.el('input-sub-color').dispatch('input');
    await h.runTimers();
    h.el('input-sub-color').value = '#00ff00';
    h.el('input-sub-color').dispatch('input');
    h.backend.release();
    await h.settle();
    assert(h.el('input-sub-color').value === '#00ff00', 'An edit typed during the in-flight save survives its adoption');
    h.backend.holdStyleSaves = false;
    await h.runTimers();
    assert(h.backend.config.subtitle_theme['--sub-color'] === '#00ff00', 'The edit typed during the save is sent by the next save');
  }
  {
    // MAJOR: a dirty general form must still adopt an external built-in style change
    const h = await loadAppHarness(styleBaseConfig);
    h.el('input-ws-port').value = '9000';
    assert(h.app.isFormDirty(), 'Precondition: the general form has an unsaved edit');
    await h.emit('config_changed', { config: { ...styleBaseConfig, subtitle_style: 'neon' }, changed_keys: ['subtitle_style'], restart_required: false });
    assert(h.el('select-subtitle-style').value === 'neon' && h.app.currentConfig.subtitle_style === 'neon',
      'An external built-in style change reaches the Style tab while the general form is dirty');
    h.el('input-sub-color').value = '#ff0000';
    h.el('input-sub-color').dispatch('input');
    await h.runTimers();
    assert(h.styleSaves().at(-1)?.args.subtitleStyle === 'neon',
      'The next style save sends the externally changed built-in style, not the stale one');
  }
  {
    // An unsent built-in pick survives an external refresh of a clean general form
    const h = await loadAppHarness(styleBaseConfig);
    assert(!h.app.isFormDirty(), `Precondition: the general form is clean (differs: ${h.app.formDiffKeys().join('; ')})`);
    h.el('select-subtitle-style').value = 'neon';
    h.el('select-subtitle-style').dispatch('change');
    await h.emit('config_changed', { config: { ...styleBaseConfig, subtitle_style: 'karaoke', ws_port: 9100 }, changed_keys: ['subtitle_style', 'ws_port'], restart_required: false });
    assert(h.el('select-subtitle-style').value === 'neon' && h.app.currentConfig.subtitle_style === 'neon',
      'An unsent built-in pick survives an external refresh of the general form');
    await h.runTimers();
    assert(h.styleSaves().at(-1)?.args.subtitleStyle === 'neon', 'The pending style save sends the user pick, not the disk value');
  }
  {
    // A failed style save shows the error; the next edit retries; a successful save clears the error and the dirty state
    const h = await loadAppHarness(styleBaseConfig);
    const statusText = () => h.el('sub-style-status').textContent;
    h.backend.failStyleSaves = 1;
    h.el('input-sub-color').value = '#ff0000';
    h.el('input-sub-color').dispatch('input');
    await h.runTimers();
    assert(statusText() === t('sub_style_save_failed'), 'A failed style save shows the error');
    h.el('input-sub-color').value = '#00ff00';
    h.el('input-sub-color').dispatch('input');
    await h.runTimers();
    assert(h.backend.config.subtitle_theme['--sub-color'] === '#00ff00' && h.app.styleDirty === false,
      'The next edit retries the save, and a successful save clears the unsent state');
    assert(statusText() === '', 'A successful save clears the error text');
  }
  {
    // Preset save, load and delete go through the real handlers; their messages survive the style save that follows
    const h = await loadAppHarness(styleBaseConfig);
    const statusText = () => h.el('sub-style-status').textContent;
    h.el('input-sub-preset-name').value = 'mine';
    h.el('btn-sub-preset-save').click();
    await h.confirmModal();
    await h.runTimers();
    assert(h.backend.config.subtitle_presets.some((preset) => preset.name === 'mine'), 'A preset save is written through the style save');
    assert(statusText() === t('sub_preset_saved'), 'The preset saved message stays visible after its style save resolves');
    h.el('select-subtitle-style').value = 'preset:mine';
    h.el('select-subtitle-style').dispatch('change');
    await h.runTimers();
    assert(statusText() === t('sub_preset_loaded'), 'The preset loaded message stays visible after its style save resolves');
    h.el('btn-sub-preset-delete').click();
    await h.confirmModal();
    await h.runTimers();
    assert(h.backend.config.subtitle_presets.length === 0 && statusText() === t('sub_preset_deleted'),
      'A preset delete is written through the style save and its message stays visible');
  }
  {
    // My styles: a saved preset applies its base style and tokens from the style selector, and saving records the base style
    const h = await loadAppHarness({ ...styleBaseConfig, subtitle_style: 'karaoke', subtitle_presets: [
      { name: 'green', style: 'neon', tokens: { '--sub-color': '#00ff00', '--sub-lifetime': '8s' } },
      { name: 'old', tokens: { '--sub-color': '#ff0000' } },
    ] }, { systemFonts: ['Bahnschrift', 'Segoe UI'] });
    const picker = h.el('select-subtitle-style');
    const choose = async (value) => { picker.value = value; picker.dispatch('change'); await h.runTimers(); };
    assert(h.backend.calls.some((call) => call.cmd === 'list_system_fonts'), 'The installed fonts are requested on load');
    await choose('preset:green');
    assert(h.backend.config.subtitle_style === 'neon' && h.backend.config.subtitle_theme['--sub-color'] === '#00ff00'
      && h.backend.config.subtitle_theme['--sub-lifetime'] === '8s' && h.el('input-sub-lifetime').value == 8 && picker.value === 'preset:green',
    'Choosing a saved style applies its base style and tokens, saves them and keeps it selected');
    await choose('preset:old');
    assert(h.backend.config.subtitle_style === 'neon' && h.backend.config.subtitle_theme['--sub-color'] === '#ff0000' && !('--sub-lifetime' in h.backend.config.subtitle_theme),
      'A preset without a base style keeps the style in use and replaces the tokens');
    await choose('bold');
    assert(h.backend.config.subtitle_style === 'bold', 'Choosing a built-in style still saves it');
    h.el('input-sub-preset-name').value = 'mine';
    h.el('btn-sub-preset-save').click();
    await h.confirmModal();
    await h.runTimers();
    assert(h.backend.config.subtitle_presets.find((preset) => preset.name === 'mine')?.style === 'bold' && picker.value === 'preset:mine',
      'Saving records the current built-in style and selects the new saved style');
    h.el('btn-sub-preset-delete').click();
    await h.confirmModal();
    await h.runTimers();
    assert(!h.backend.config.subtitle_presets.some((preset) => preset.name === 'mine') && picker.value === 'bold',
      'Deleting the selected saved style returns the selector to its base style');
    h.el('select-sub-font').value = 'Bahnschrift';
    h.el('select-sub-font').dispatch('input');
    await h.runTimers();
    assert(h.backend.config.subtitle_theme['--sub-font-family'] === 'Bahnschrift', 'An installed font is saved in the theme');
  }
  {
    // Choosing a built-in style after a saved style replaces the whole theme, like "Restablecer estilo"
    const h = await loadAppHarness({ ...styleBaseConfig, subtitle_presets: [
      { name: 'Plynte', tokens: { '--sub-font-family': 'Bahnschrift', '--sub-font-size': '40px', '--sub-color': '#00ff00' } },
    ] }, { systemFonts: ['Bahnschrift', 'Segoe UI'] });
    const picker = h.el('select-subtitle-style');
    const choose = async (value) => { picker.value = value; picker.dispatch('change'); await h.runTimers(); };
    await choose('preset:Plynte');
    assert(h.backend.config.subtitle_theme['--sub-color'] === '#00ff00' && h.el('input-sub-preset-name').value === 'Plynte',
      'Precondition: the saved style is loaded');
    await choose('neon');
    assert(h.backend.config.subtitle_style === 'neon' && Object.keys(h.backend.config.subtitle_theme).length === 0,
      'Choosing a built-in style after a saved style saves an empty theme');
    assert(h.el('select-sub-font').value !== 'Bahnschrift' && h.el('input-sub-color').value !== '#00ff00' && h.el('input-sub-preset-name').value === '',
      'Choosing a built-in style resets the controls and clears the preset name');
  }
  for (const reducedMotion of [false, true]) {
    // Preview Play replays enter, hold and exit on the preview line, then restores the static preview; reduced motion skips it
    const h = await loadAppHarness({ ...styleBaseConfig, subtitle_style: 'karaoke' }, { reducedMotion });
    const line = h.el('sub-style-preview-text');
    const log = [];
    line.classList = { add: (name) => log.push(`+${name}`), remove: (name) => log.push(`-${name}`), toggle() {}, contains() { return false; } };
    h.el('btn-sub-preview-play').click();
    await h.runTimers();
    if (reducedMotion) {
      assert(log.length === 0 && /\bvisible\b/.test(line.className),
        'Reduced motion keeps the preview static: no enter or exit is played');
    } else {
      assert(log.join(' ') === '-visible +visible -visible +hiding' && /\bvisible\b/.test(line.className) && !/hiding/.test(line.className),
        `Play runs enter, hold and exit, then shows the static preview again (saw: ${log.join(' ')})`);
    }
  }
  {
    // A general save after a built-in pick keeps the pick: the form copy is sent, and the saved config keeps it
    const h = await loadAppHarness(styleBaseConfig);
    h.el('select-subtitle-style').value = 'neon';
    h.el('select-subtitle-style').dispatch('change');
    await h.runTimers();
    h.el('input-ws-port').value = '9000';
    h.el('btn-save-config').click();
    await h.settle();
    const generalSave = h.backend.calls.filter((call) => call.cmd === 'save_config').at(-1);
    assert(generalSave?.args.config.subtitle_style === 'neon', 'A general save sends the built-in style picked in the Style tab');
    assert(h.backend.config.subtitle_style === 'neon' && h.app.currentConfig.subtitle_style === 'neon',
      'A general save after a built-in pick keeps neon on disk and in the client');
  }
  {
    // F5: the letter spacing control is kept after its save resolves, shows its readout, and is sent as --sub-letter-spacing
    const h = await loadAppHarness(styleBaseConfig);
    h.el('input-sub-letter-spacing').value = '3';
    h.el('input-sub-letter-spacing').dispatch('input');
    await h.runTimers();
    assert(h.styleSaves().at(-1)?.args.theme['--sub-letter-spacing'] === '3px', 'A letter spacing change is sent as --sub-letter-spacing');
    assert(Number(h.el('input-sub-letter-spacing').value) === 3 && h.el('val-sub-letter-spacing').textContent === '3px'
      && h.app.currentConfig.subtitle_theme['--sub-letter-spacing'] === '3px',
      'The saved letter spacing stays on screen with its readout and in the config');
  }
  {
    // Word spacing: the word spacing control is kept after its save resolves, shows its readout, and is sent as --sub-word-spacing
    const h = await loadAppHarness(styleBaseConfig);
    h.el('input-sub-word-spacing').value = '4';
    h.el('input-sub-word-spacing').dispatch('input');
    await h.runTimers();
    assert(h.styleSaves().at(-1)?.args.theme['--sub-word-spacing'] === '4px', 'A word spacing change is sent as --sub-word-spacing');
    assert(Number(h.el('input-sub-word-spacing').value) === 4 && h.el('val-sub-word-spacing').textContent === '4px'
      && h.app.currentConfig.subtitle_theme['--sub-word-spacing'] === '4px',
      'The saved word spacing stays on screen with its readout and in the config');
  }

  // 16. F5 letter spacing: the control maps both ways, presets keep it, and the shared fixture covers it
  {
    const { themeFromControls, controlsFromTheme, savePreset, loadPresetTheme } = subtitleStyle;
    assert(themeFromControls({ letterSpacing: 2.5 })['--sub-letter-spacing'] === '2.5px', 'A letter spacing control is written as --sub-letter-spacing in px');
    assert(!('--sub-letter-spacing' in themeFromControls({ letterSpacing: 0 })), 'Letter spacing 0 (normal) is not written');
    assert(!('--sub-letter-spacing' in themeFromControls({ letterSpacing: 0 }, { '--sub-letter-spacing': '3px' })),
      'Letter spacing 0 drops a stale token from the theme');
    assert(themeFromControls({ letterSpacing: 99 })['--sub-letter-spacing'] === '10px', 'Letter spacing is clamped to the control range');
    assert(controlsFromTheme({ '--sub-letter-spacing': '-2px' }).letterSpacing === -2, 'A negative token reads back into the control');
    assert(controlsFromTheme({ '--sub-letter-spacing': 'normal' }).letterSpacing === 0, 'The token normal reads back as 0');
    assert(controlsFromTheme({}).letterSpacing === 0, 'A missing token reads back as 0');
    assert(controlsFromTheme({ '--sub-letter-spacing': '50px' }).letterSpacing === 10, 'An out-of-range token is clamped to the control range');
    const preset = savePreset([], 'tight', themeFromControls({ letterSpacing: 3 })).presets[0];
    assert(preset.tokens['--sub-letter-spacing'] === '3px' && loadPresetTheme(preset).controls.letterSpacing === 3,
      'Letter spacing survives a preset save and load');
    assert(['normal', '2px', '-2px'].every((value) => parityCases.valid.some(([token, v]) => token === '--sub-letter-spacing' && v === value)),
      'The shared fixture has valid letter spacing cases for the range the control can write');
    const styleSection = htmlContent.slice(htmlContent.indexOf('id="tab-style"'), htmlContent.indexOf('id="tab-advanced"'));
    assert(styleSection.includes('id="input-sub-letter-spacing"') && styleSection.includes('data-i18n="lbl_sub_letter_spacing"'),
      'The letter spacing control is in the Style tab with a translated label');
  }
  {
    const { themeFromControls, controlsFromTheme, savePreset, loadPresetTheme, SUBTITLE_WORD_SPACING, SUBTITLE_STYLE_DEFAULTS } = subtitleStyle;
    assert(SUBTITLE_WORD_SPACING && SUBTITLE_WORD_SPACING.min === -2 && SUBTITLE_WORD_SPACING.max === 20 && SUBTITLE_WORD_SPACING.step === 0.5,
      'SUBTITLE_WORD_SPACING is exported with min -2, max 20, step 0.5');
    assert(SUBTITLE_STYLE_DEFAULTS.wordSpacing === 0, 'SUBTITLE_STYLE_DEFAULTS includes wordSpacing: 0');
    assert(themeFromControls({ wordSpacing: 3.5 })['--sub-word-spacing'] === '3.5px', 'A word spacing control is written as --sub-word-spacing in px');
    assert(!('--sub-word-spacing' in themeFromControls({ wordSpacing: 0 })), 'Word spacing 0 (normal) is not written');
    assert(!('--sub-word-spacing' in themeFromControls({ wordSpacing: 0 }, { '--sub-word-spacing': '4px' })),
      'Word spacing 0 drops a stale token from the theme');
    assert(themeFromControls({ wordSpacing: 99 })['--sub-word-spacing'] === '20px', 'Word spacing is clamped to the control max range');
    assert(themeFromControls({ wordSpacing: -10 })['--sub-word-spacing'] === '-2px', 'Word spacing is clamped to the control min range');
    assert(controlsFromTheme({ '--sub-word-spacing': '-1.5px' }).wordSpacing === -1.5, 'A negative word spacing token reads back into the control');
    assert(controlsFromTheme({ '--sub-word-spacing': 'normal' }).wordSpacing === 0, 'The word spacing token normal reads back as 0');
    assert(controlsFromTheme({}).wordSpacing === 0, 'A missing word spacing token reads back as 0');
    assert(controlsFromTheme({ '--sub-word-spacing': '50px' }).wordSpacing === 20, 'An out-of-range word spacing token is clamped to the control range');
    const preset = savePreset([], 'spaced', themeFromControls({ wordSpacing: 5 })).presets[0];
    assert(preset.tokens['--sub-word-spacing'] === '5px' && loadPresetTheme(preset).controls.wordSpacing === 5,
      'Word spacing survives a preset save and load');
    const styleSection = htmlContent.slice(htmlContent.indexOf('id="tab-style"'), htmlContent.indexOf('id="tab-advanced"'));
    assert(styleSection.includes('id="input-sub-word-spacing"') && styleSection.includes('data-i18n="lbl_sub_word_spacing"'),
      'The word spacing control is in the Style tab with a translated label');
  }
  {
    // F6: the preset buttons use the existing secondary compact classes, which have CSS rules (`.btn` has none)
    const styleSection = htmlContent.slice(htmlContent.indexOf('id="tab-style"'), htmlContent.indexOf('id="tab-advanced"'));
    const cssText = fs.readFileSync(path.join(__dirname, 'src', 'styles.css'), 'utf8');
    for (const id of ['btn-sub-preset-save', 'btn-sub-preset-delete']) {
      const tag = styleSection.match(new RegExp(`<button[^>]*id="${id}"[^>]*>`))?.[0] ?? '';
      const classes = (tag.match(/class="([^"]*)"/)?.[1] ?? '').split(/\s+/);
      assert(classes.includes('btn-secondary') && classes.includes('btn-compact') && !classes.includes('btn'),
        `${id} uses the existing secondary compact button classes`);
    }
    // Report only: classes in the Style tab with no CSS rule (not a failure, so the report stays visible)
    const styleClasses = [...new Set([...styleSection.matchAll(/class="([^"]*)"/g)].flatMap((match) => match[1].split(/\s+/)).filter(Boolean))];
    const unstyled = styleClasses.filter((name) => !new RegExp(`\\.${name}(?![\\w-])`).test(cssText));
    console.log(`[INFO] Style tab classes with no CSS rule: ${unstyled.join(', ') || 'none'}`);
  }

  // 13b. U5: Style tab cards, reset/save confirmations, unsaved-changes reminder, header badge, OBS tip
  console.log('\n--- Verifying Style tab cards, confirmations and unsaved reminder (U5) ---');
  {
    const section = htmlContent.slice(htmlContent.indexOf('id="tab-style"'), htmlContent.indexOf('id="tab-advanced"'));
    const cardBody = (id) => {
      const start = section.indexOf(`id="${id}"`);
      if (start === -1) return '';
      const next = section.indexOf('id="card-style-', start + 1);
      return section.slice(start, next === -1 ? undefined : next);
    };
    const expected = {
      'card-style-choose': ['title_style_card_style', 'select-subtitle-style', 'input-sub-preset-name', 'btn-sub-preset-save', 'btn-sub-preset-delete', 'btn-sub-style-reset', 'chip-sub-style-unsaved'],
      'card-style-preview': ['title_style_card_preview', 'sub-style-preview', 'btn-sub-preview-play'],
      'card-style-text': ['title_style_card_text', 'select-sub-font', 'input-sub-font-size', 'select-sub-weight', 'input-sub-color', 'input-sub-letter-spacing', 'input-sub-word-spacing'],
      'card-style-background': ['title_style_card_background', 'check-sub-bg-custom', 'input-sub-bg', 'input-sub-bg-opacity'],
      'card-style-animation': ['title_style_card_animation', 'select-sub-animation', 'input-sub-anim-duration', 'input-sub-lifetime'],
    };
    for (const [card, needles] of Object.entries(expected)) {
      const body = cardBody(card);
      assert(new RegExp(`<div class="card style-card" id="${card}"`).test(section) && needles.every((needle) => body.includes(needle)),
        `Style tab card #${card} is a titled card holding its controls`);
    }
    assert(!section.includes('checkbox-label') && /<label class="switch-label"[^>]*>\s*<span class="switch-track"><input type="checkbox" id="check-sub-bg-custom" class="switch-input"><span class="switch-slider"><\/span><\/span>/.test(section),
      'Custom background uses the Advanced-tab toggle switch markup');
    assert(/id="chip-sub-style-unsaved"[^>]*\bhidden\b/.test(section) && /id="chip-sub-style-unsaved"[^>]*role="status"/.test(section),
      'The unsaved-changes chip starts hidden and is a status region');
    const keys = ['title_style_card_style', 'title_style_card_preview', 'title_style_card_text', 'title_style_card_background', 'title_style_card_animation',
      'btn_sub_style_reset', 'sub_style_unsaved', 'sub_style_reset_done', 'sub_reset_title', 'sub_reset_message', 'sub_reset_confirm',
      'sub_save_title', 'sub_save_message', 'sub_save_confirm', 'sub_delete_title', 'sub_delete_message', 'sub_delete_confirm',
      'sub_unsaved_title', 'sub_unsaved_message', 'btn_sub_unsaved_save', 'btn_sub_unsaved_discard', 'badge_info_label'];
    const missing = keys.filter((key) => !i18nModule.TRANSLATIONS.es?.[key] || !i18nModule.TRANSLATIONS.en?.[key]);
    assert(missing.length === 0, `U5 strings exist in es and en (missing: ${missing.join(', ') || 'none'})`);
  }
  {
    // Reset style: confirm first; then the theme goes back to the defaults and the selected built-in style stays
    const h = await loadAppHarness({ ...styleBaseConfig, subtitle_style: 'neon' });
    h.el('modal-container').hidden = true;
    const modalOpen = () => h.el('modal-container').hidden === false;
    h.el('input-sub-color').value = '#ff0000';
    h.el('input-sub-color').dispatch('input');
    h.el('input-sub-font-size').value = '60';
    h.el('input-sub-font-size').dispatch('input');
    await h.runTimers();
    const savesBefore = h.styleSaves().length;
    h.el('btn-sub-style-reset').click();
    await h.settle();
    assert(modalOpen() && h.el('modal-title').textContent === t('sub_reset_title') && h.el('modal-btn-confirm').textContent === t('sub_reset_confirm'),
      'Reset style asks for confirmation first');
    h.cancelModal();
    await h.runTimers();
    assert(!modalOpen() && h.el('input-sub-color').value === '#ff0000' && h.styleSaves().length === savesBefore,
      'Cancelling the reset changes and saves nothing');
    h.el('btn-sub-style-reset').click();
    await h.settle();
    await h.confirmModal();
    await h.runTimers();
    assert(JSON.stringify(h.backend.config.subtitle_theme) === '{}' && h.backend.config.subtitle_style === 'neon'
      && h.el('select-subtitle-style').value === 'neon' && h.el('input-sub-color').value === '#ffffff' && h.el('input-sub-font-size').value == 42,
    'Confirmed reset saves the default theme and keeps the selected built-in style');
    assert(h.el('sub-style-status').textContent === t('sub_style_reset_done'), 'The reset shows a short confirmation message');
  }
  {
    // Save and delete of a saved style ask first
    const h = await loadAppHarness(styleBaseConfig);
    h.el('modal-container').hidden = true;
    const modalOpen = () => h.el('modal-container').hidden === false;
    h.el('input-sub-preset-name').value = 'mine';
    h.el('btn-sub-preset-save').click();
    await h.settle();
    assert(modalOpen() && h.el('modal-title').textContent === t('sub_save_title') && h.el('modal-message').textContent.includes('mine'),
      'Save asks for confirmation and names the style');
    h.cancelModal();
    await h.runTimers();
    assert(h.backend.config.subtitle_presets.length === 0 && h.app.currentConfig.subtitle_presets.length === 0, 'Cancelling the save stores nothing');
    h.el('btn-sub-preset-save').click();
    await h.settle();
    await h.confirmModal();
    await h.runTimers();
    assert(h.backend.config.subtitle_presets.some((preset) => preset.name === 'mine'), 'Confirming the save stores the style');
    h.el('select-subtitle-style').value = 'preset:mine';
    h.el('select-subtitle-style').dispatch('change');
    await h.runTimers();
    h.el('btn-sub-preset-delete').click();
    await h.settle();
    assert(modalOpen() && h.el('modal-title').textContent === t('sub_delete_title') && h.el('modal-message').textContent.includes('mine') && h.backend.config.subtitle_presets.length === 1,
      'Delete asks for confirmation and removes nothing yet');
    h.cancelModal();
    await h.runTimers();
    assert(h.backend.config.subtitle_presets.length === 1, 'Cancelling the delete keeps the style');
  }
  {
    // Unsaved reminder: only a saved style can differ from what is stored (the live theme auto-saves)
    const h = await loadAppHarness({ ...styleBaseConfig, subtitle_style: 'karaoke', subtitle_presets: [{ name: 'green', style: 'neon', tokens: { '--sub-color': '#00ff00' } }] });
    h.el('modal-container').hidden = true;
    const chipHidden = () => h.el('chip-sub-style-unsaved').hidden;
    const edit = async (id, value) => { h.el(id).value = value; h.el(id).dispatch('input'); await h.runTimers(); };
    const choose = async (value) => { h.el('select-subtitle-style').value = value; h.el('select-subtitle-style').dispatch('change'); await h.runTimers(); };
    assert(chipHidden() === true && h.app.styleChangesUnsaved() === false, 'No unsaved chip on load');
    await edit('input-sub-color', '#123456');
    assert(chipHidden() === true, 'Edits on a built-in style auto-save, so there is no unsaved chip');
    await choose('preset:green');
    assert(chipHidden() === true, 'Choosing a saved style shows no unsaved chip');
    await edit('input-sub-color', '#ff0000');
    assert(chipHidden() === false && h.app.styleChangesUnsaved() === true, 'Editing a saved style shows the unsaved chip');
    await edit('input-sub-color', '#00ff00');
    assert(chipHidden() === true, 'Putting the value back hides the unsaved chip');
    await edit('input-sub-color', '#ff0000');
    await choose('bold');
    assert(chipHidden() === true, 'Switching to a built-in style hides the unsaved chip');

    // Leaving the tab: Save keeps the edits in the saved style; Discard restores it
    await choose('preset:green');
    await edit('input-sub-color', '#ff0000');
    const leave = h.app.confirmLeaveStyleTab();
    await h.settle();
    assert(h.el('modal-container').hidden === false && h.el('modal-title').textContent === t('sub_unsaved_title') && h.el('modal-message').textContent.includes('green')
      && h.el('modal-btn-confirm').textContent === t('btn_sub_unsaved_save') && h.el('modal-btn-cancel').textContent === t('btn_sub_unsaved_discard'),
    'Leaving the Style tab with unsaved changes offers Save or Discard');
    await h.confirmModal();
    assert(await leave === true, 'Save lets the user leave');
    await h.runTimers();
    assert(h.backend.config.subtitle_presets[0].tokens['--sub-color'] === '#ff0000' && chipHidden() === true, 'Save stores the edits in the saved style and hides the chip');
    await edit('input-sub-color', '#0000ff');
    const discard = h.app.confirmLeaveStyleTab();
    await h.settle();
    h.cancelModal();
    assert(await discard === true, 'Discard lets the user leave');
    await h.runTimers();
    assert(h.el('input-sub-color').value === '#ff0000' && h.backend.config.subtitle_theme['--sub-color'] === '#ff0000' && chipHidden() === true
      && h.backend.config.subtitle_presets[0].tokens['--sub-color'] === '#ff0000',
    'Discard restores the saved style on screen and in the live theme');
    h.el('modal-container').hidden = true;
    assert(await h.app.confirmLeaveStyleTab() === true && h.el('modal-container').hidden === true, 'With nothing unsaved, leaving does not ask');
  }
  {
    // Tab navigation can veto leaving a tab; the close guard shares the same check
    const panes = ['a', 'b'].map((id) => ({ id: `pane-${id}`, hidden: true, classList: { toggle() {} } }));
    const mkTab = (pane, active) => ({ attributes: { 'aria-controls': pane.id }, handlers: {}, tabIndex: -1, classList: { values: new Set(active ? ['active'] : []), add() {}, remove() {}, contains(v) { return this.values.has(v); }, toggle(v, f) { if (f) this.values.add(v); else this.values.delete(v); } }, getAttribute(n) { return this.attributes[n]; }, setAttribute(n, v) { this.attributes[n] = v; }, addEventListener(n, cb) { this.handlers[n] = cb; }, focus() {} });
    const tabs = [mkTab(panes[0], true), mkTab(panes[1], false)];
    const list = { ownerDocument: { getElementById: (id) => panes.find((pane) => pane.id === id) }, querySelectorAll: () => tabs };
    let allow = false;
    const asked = [];
    bindTabNavigation(list, { canLeave: async (from, to) => { asked.push([from, to]); return allow; } });
    await tabs[1].handlers.click();
    await new Promise((resolve) => setImmediate(resolve));
    assert(asked.length === 1 && asked[0][0] === tabs[0] && asked[0][1] === tabs[1] && tabs[0].attributes['aria-selected'] === 'true' && panes[1].hidden === true,
      'canLeave can keep the user on the current tab');
    allow = true;
    await tabs[1].handlers.click();
    await new Promise((resolve) => setImmediate(resolve));
    assert(tabs[1].attributes['aria-selected'] === 'true' && panes[1].hidden === false, 'canLeave allowing the move switches tabs');
    const appSource = fs.readFileSync(path.join(__dirname, 'src', 'app.js'), 'utf8');
    assert(/bindTabNavigation\(document\.querySelector\('\.tab-nav'\), \{[^}]*?canLeave:/.test(appSource)
      && /onCloseRequested[\s\S]{0,500}styleChangesUnsaved\(\)/.test(appSource),
    'The tab bar and the window close guard both check unsaved style changes');
  }
  {
    // Header version badge: version, label and chevron in one About button
    const headerHtml = htmlContent.match(/<header[\s\S]*?<\/header>/)?.[0] || '';
    const badge = headerHtml.match(/<button[^>]*id="btn-about"[\s\S]*?<\/button>/)?.[0] || '';
    assert(badge.includes('data-i18n="app_version"') && badge.includes('data-i18n="badge_info_label"') && badge.includes('lucide-chevron-right')
      && /aria-label="About LiveAudio"/.test(badge) && badge.includes('data-i18n-aria-label="btn_about"'),
    'The version badge shows the version, the info label and a chevron, and keeps its accessible name');
    assert(i18nModule.TRANSLATIONS.es.badge_info_label === 'Info y estadísticas' && i18nModule.TRANSLATIONS.en.badge_info_label === 'Info & stats',
      'The badge label is localized');
    const css = fs.readFileSync(path.join(__dirname, 'src', 'styles.css'), 'utf8');
    assert(/button\.brand-badge:focus-visible/.test(css) && /button\.brand-badge:hover/.test(css) && css.includes('.brand-badge-more'),
      'The badge has hover, focus and label styles');
    assert(fs.readFileSync(path.join(__dirname, '..', 'THIRD_PARTY_NOTICES.md'), 'utf8').includes('chevron-right'), 'THIRD_PARTY_NOTICES lists the chevron-right icon');
  }
  {
    // OBS troubleshooting tip
    assert(i18nModule.TRANSLATIONS.es.obs_instructions_html.includes('Actualizar la caché de la página actual')
      && i18nModule.TRANSLATIONS.en.obs_instructions_html.includes('Refresh cache of current page'),
    'OBS instructions tell users to refresh the Browser Source cache when it works in a browser but not in OBS (es + en)');
    assert(htmlContent.includes('Actualizar la caché de la página actual'), 'The default OBS instructions in index.html carry the cache tip');
    assert(fs.readFileSync(path.join(__dirname, '..', 'docs', 'WEBSOCKET_OBS.md'), 'utf8').includes('Refresh cache of current page'),
      'docs/WEBSOCKET_OBS.md troubleshooting lists the cache refresh');
  }

  // 13c. U6: Advanced tab cards, toggles, reset to defaults, unsaved reminder
  console.log('\n--- Verifying Advanced tab cards, reset and unsaved reminder (U6) ---');
  {
    const advHtml = htmlContent.slice(htmlContent.indexOf('id="tab-advanced"'), htmlContent.indexOf('id="tab-files"'));
    const cardBody = (id) => {
      const start = advHtml.indexOf(`id="${id}"`);
      if (start === -1) return '';
      const next = advHtml.indexOf('id="card-adv-', start + 1);
      return advHtml.slice(start, next === -1 ? undefined : next);
    };
    const expected = {
      'card-adv-engine': ['title_adv_engine', 'select-model-size', 'select-compute-device', 'slider-cpu-threads', 'select-asr-backend', 'engine-status-pill', 'engine-technical-details', 'btn-runtime-repair'],
      'card-adv-voice': ['title_adv_voice', 'slider-silence-timeout', 'slider-max-chunk', 'slider-vad-pad', 'slider-vad-threshold', 'check-rnnoise-enabled'],
      'card-adv-subtitles': ['title_adv_subtitles', 'check-live-partial-subtitles-enabled', 'select-backlog-policy', 'slider-max-live-delay', 'slider-catchup-interval'],
      'card-adv-vocab': ['title_adv_vocab', 'input-blacklist', 'prompt-es'],
      'card-adv-network': ['title_adv_network', 'input-ws-port', 'check-acoustic-events-enabled', 'check-mcp-style-enabled', 'check-mcp-system-enabled'],
      'card-adv-updates': ['title_adv_updates', 'check-updates-enabled', 'btn-check-now', 'card-release-notes'],
    };
    for (const [card, needles] of Object.entries(expected)) {
      const body = cardBody(card);
      assert(new RegExp(`<div class="card adv-card" id="${card}"`).test(advHtml) && needles.every((needle) => body.includes(needle)),
        `Advanced card #${card} is a titled card holding its controls`);
    }
    const boxes = [...advHtml.matchAll(/<input type="checkbox"[^>]*>/g)].map((m) => m[0]);
    assert(boxes.length === 6 && boxes.every((tag) => tag.includes('class="switch-input"')) && !advHtml.includes('checkbox-label'),
      'Every Advanced checkbox is a real checkbox styled as the toggle switch');
    assert((advHtml.match(/class="badge-restart"/g) || []).length >= 4 && advHtml.includes('data-i18n="badge_restart"'), 'Restart-required settings carry the restart badge');
    assert(/id="chip-adv-unsaved"[^>]*\bhidden\b/.test(advHtml) && /id="chip-adv-unsaved"[^>]*role="status"/.test(advHtml) && !advHtml.includes('id="btn-adv-reset"'),
      'The Advanced unsaved chip starts hidden and the footer Reset covers Advanced');
    const css = fs.readFileSync(path.join(__dirname, 'src', 'styles.css'), 'utf8');
    assert(/\.adv-card/.test(css) && /\.badge-restart/.test(css), 'Advanced cards and restart badge have CSS rules');
    const keys = ['title_adv_engine', 'title_adv_voice', 'title_adv_subtitles', 'title_adv_vocab', 'title_adv_network', 'title_adv_updates', 'badge_restart',
      'adv_unsaved', 'tab_reset_title', 'tab_reset_message', 'tab_reset_confirm', 'tab_reset_done',
      'adv_unsaved_title', 'adv_unsaved_message', 'btn_adv_unsaved_save', 'btn_adv_unsaved_discard'];
    const missing = keys.filter((key) => !i18nModule.TRANSLATIONS.es?.[key] || !i18nModule.TRANSLATIONS.en?.[key]);
    assert(missing.length === 0, `U6 strings exist in es and en (missing: ${missing.join(', ') || 'none'})`);
    const defaults = runtimeSettings.advancedDefaults(8);
    assert(defaults.ws_port === 8765 && defaults.vad_speech_pad_ms === 200 && defaults.silence_timeout === 0.5 && defaults.cpu_threads === 4
      && defaults.rnnoise_enabled === false && defaults.mcp_style_enabled === false && defaults.mcp_system_enabled === false && defaults.check_updates_enabled === true
      && !('subtitle_theme' in defaults) && !('subtitle_presets' in defaults) && !('whisper_context_prompt_es' in defaults),
    'advancedDefaults holds only Advanced keys (no styles, no vocabulary)');
  }
  {
    const base = { ...styleBaseConfig, rnnoise_enabled: true, ws_port: 9000, silence_timeout: 1.5, mcp_style_enabled: true, mcp_system_enabled: true, subtitle_style: 'neon', subtitle_theme: { '--sub-color': '#123456' } };
    const h = await loadAppHarness(base);
    h.el('modal-container').hidden = true;
    const modalOpen = () => h.el('modal-container').hidden === false;
    const edit = async (id, value) => { h.el(id).value = value; h.el(id).dispatch('input'); await h.settle(); };
    const writes = () => h.backend.calls.filter((c) => c.cmd === 'save_config' || c.cmd === 'reset_config_defaults').length;
    assert(h.el('chip-adv-unsaved').hidden === true && h.app.advancedChangesUnsaved() === false, 'No Advanced unsaved chip on load');
    await edit('slider-vad-pad', '300');
    assert(h.el('chip-adv-unsaved').hidden === false && h.app.advancedChangesUnsaved() === true, 'Editing an Advanced control shows the unsaved chip');
    await edit('slider-vad-pad', '200');
    assert(h.el('chip-adv-unsaved').hidden === true, 'Putting the value back hides the chip');

    // Reset to defaults: confirm first, form only, never styles; Save applies it
    const writesBefore = writes();
    h.app.setActiveTab('tab-advanced');
    h.el('btn-reset-config').click();
    await h.settle();
    assert(modalOpen() && h.el('modal-title').textContent.includes(t('tab_advanced')) && h.el('modal-btn-confirm').textContent === t('tab_reset_confirm')
      && h.el('modal-message').textContent.includes(t('lbl_ws_port')) && !h.el('modal-message').textContent.includes(t('lbl_display_mode')),
    'Reset asks for confirmation first, naming the tab and its settings');
    h.cancelModal();
    await h.settle();
    assert(h.el('input-ws-port').value == 9000 && h.el('check-rnnoise-enabled').checked === true, 'Cancelling the reset changes nothing');
    h.el('btn-reset-config').click();
    await h.settle();
    await h.confirmModal();
    assert(h.el('input-ws-port').value == 8765 && h.el('check-rnnoise-enabled').checked === false && h.el('check-mcp-style-enabled').checked === false && h.el('check-mcp-system-enabled').checked === false
      && h.el('slider-silence-timeout').value == 0.5 && h.el('val-silence-timeout').textContent === '0.5s',
    'Confirmed reset puts the Advanced controls back to their defaults');
    assert(h.el('chip-adv-unsaved').hidden === false, 'The reset leaves the changes unsaved until Save');
    assert(!h.backend.calls.some((c) => c.cmd === 'reset_config_defaults'), 'The footer Reset never calls the whole-config reset');
    assert(writes() === writesBefore && h.app.currentConfig.subtitle_style === 'neon' && h.app.currentConfig.subtitle_theme['--sub-color'] === '#123456',
      'The reset writes nothing by itself and leaves the saved styles alone');

    // Leaving the tab: Save writes, Discard restores
    const leave = h.app.confirmLeaveAdvancedTab();
    await h.settle();
    assert(modalOpen() && h.el('modal-title').textContent === t('adv_unsaved_title')
      && h.el('modal-btn-confirm').textContent === t('btn_adv_unsaved_save') && h.el('modal-btn-cancel').textContent === t('btn_adv_unsaved_discard'),
    'Leaving the Advanced tab with unsaved changes offers Save or Discard');
    await h.confirmModal();
    assert(await leave === true && h.backend.config.ws_port === 8765 && h.backend.config.rnnoise_enabled === false && h.el('chip-adv-unsaved').hidden === true
      && h.backend.config.subtitle_theme['--sub-color'] === '#123456',
    'Save stores the Advanced changes, hides the chip and keeps the styles');
    await edit('input-ws-port', '9100');
    const discard = h.app.confirmLeaveAdvancedTab();
    await h.settle();
    h.cancelModal();
    assert(await discard === true && h.el('input-ws-port').value == 8765 && h.el('chip-adv-unsaved').hidden === true && h.backend.config.ws_port === 8765,
      'Discard restores the saved Advanced values without saving');
    h.el('modal-container').hidden = true;
    assert(await h.app.confirmLeaveAdvancedTab() === true && h.el('modal-container').hidden === true, 'With nothing unsaved, leaving does not ask');
    const appSource = fs.readFileSync(path.join(__dirname, 'src', 'app.js'), 'utf8');
    assert(/canLeave:[\s\S]{0,300}tab-advanced-tab[\s\S]{0,100}confirmLeaveAdvancedTab/.test(appSource) && /onCloseRequested[\s\S]{0,700}advancedChangesUnsaved\(\)/.test(appSource),
      'The tab bar and the window close guard both check unsaved Advanced changes');
  }

  // Footer Reset is limited to the active tab
  {
    const base = { ...styleBaseConfig, language: 'en', subtitle_display_mode: 'ribbon', subtitle_ribbon_max_lines: 5, continuous_session: false,
      save_transcript_enabled: false, save_vtt_enabled: false, ws_port: 9000, subtitle_style: 'neon', subtitle_theme: { '--sub-color': '#123456' } };
    const h = await loadAppHarness(base);
    h.el('modal-container').hidden = true;
    const modalOpen = () => h.el('modal-container').hidden === false;
    const reset = async () => { h.el('btn-reset-config').click(); await h.settle(); };
    const tabs = runtimeSettings.TAB_RESETS;
    assert(Object.keys(tabs).sort().join() === 'tab-advanced,tab-files,tab-subtitles', 'Only Subtitles, Advanced and Files have a footer Reset (Home and Style do not)');
    h.app.setActiveTab('tab-setup');
    assert(h.el('btn-reset-config').hidden === true, 'Home hides the footer Reset');
    assert(/\.tabs-footer \[hidden\]\s*\{\s*display:\s*none/.test(fs.readFileSync(path.join(__dirname, 'src', 'styles.css'), 'utf8')), 'A hidden footer button is really hidden by CSS');
    h.app.setActiveTab('tab-style');
    assert(h.el('btn-reset-config').hidden === true, 'Style hides the footer Reset (it has its own Reset style)');
    await reset();
    assert(!modalOpen(), 'A hidden footer Reset opens nothing on Home and Style');
    h.app.setActiveTab('tab-subtitles');
    assert(h.el('btn-reset-config').hidden === false, 'Subtitles shows the footer Reset');
    await reset();
    assert(modalOpen() && h.el('modal-title').textContent.includes(t('tab_subtitles')) && h.el('modal-message').textContent.includes(t('lbl_ribbon_lines')),
      'The panel is titled with the tab and lists its settings');
    h.cancelModal(); await h.settle();
    assert(h.el('select-display-mode').value === 'ribbon', 'Cancel changes nothing');
    await reset(); await h.confirmModal();
    assert(h.el('select-display-mode').value === 'adaptive' && h.el('slider-ribbon-lines').value === 3 && h.el('val-ribbon-lines').textContent === 3
      && h.el('check-continuous').checked === false && h.el('input-ws-port').value == 9000,
    'Subtitles reset puts only its own controls back (Files and Advanced untouched)');
    h.app.setActiveTab('tab-files');
    await reset(); await h.confirmModal();
    assert(h.el('check-continuous').checked === true && h.el('check-save-transcript').checked === true && h.el('check-save-vtt').checked === true
      && h.el('input-ws-port').value == 9000, 'Files reset restores only the Files controls');
    assert(!h.backend.calls.some((c) => c.cmd === 'reset_config_defaults' || c.cmd === 'save_config'), 'Resets only fill the form; nothing is saved until Save');
    assert(h.app.currentConfig.language === 'en' && h.app.currentConfig.subtitle_style === 'neon' && h.app.currentConfig.subtitle_theme['--sub-color'] === '#123456',
      'Language, styles and saved styles are never reset');
    const keys = ['tab_reset_title', 'tab_reset_message', 'tab_reset_confirm', 'tab_reset_done'];
    assert(keys.every((k) => i18nModule.TRANSLATIONS.es?.[k] && i18nModule.TRANSLATIONS.en?.[k]), 'Footer Reset strings exist in es and en');
    const labels = Object.values(tabs).flatMap((tab) => [tab.title, ...tab.labels]);
    assert(labels.every((k) => i18nModule.TRANSLATIONS.es?.[k] && i18nModule.TRANSLATIONS.en?.[k]), 'Every Reset list label is translated');
  }

  // 14. In-App Updater Verification (T4)
  console.log('\n--- Verifying In-App Updater (T4) ---');

  // HTML elements
  assert(htmlContent.includes('id="group-updates"'), '#group-updates container exists in HTML');
  assert(htmlContent.includes('id="check-updates-enabled"'), '#check-updates-enabled switch exists');
  assert(htmlContent.includes('id="btn-check-now"'), '#btn-check-now button exists');
  assert(htmlContent.includes('id="badge-header-update"'), '#badge-header-update pill exists in header');
  assert(/\.badge-header-update\[hidden\]\s*\{\s*display:\s*none/.test(cssContent), 'hidden update pill is not shown (display override)');
  assert(appSource.includes("t('update_pill').replace('{version}'"), 'update pill text is localized Update vX');
  assert(htmlContent.includes('id="card-release-notes"'), '#card-release-notes panel exists');
  assert(htmlContent.includes('id="btn-install-now"'), '#btn-install-now button exists');
  assert(htmlContent.includes('id="btn-install-on-close"'), '#btn-install-on-close button exists');
  assert(htmlContent.includes('id="status-update-busy"'), '#status-update-busy deferred hint exists');

  // HTML adjacency: group-updates inside tab-advanced adjacent to group-runtime-maintenance
  const advancedTabHtml = htmlContent.match(/<section id="tab-advanced"[\s\S]*?<\/section>/)?.[0] || '';
  assert(advancedTabHtml.includes('id="group-updates"'), '#group-updates is inside #tab-advanced');
  assert(advancedTabHtml.includes('id="group-runtime-maintenance"'), '#group-runtime-maintenance is inside #tab-advanced');
  assert(/id="group-runtime-maintenance"[\s\S]*?id="group-updates"/.test(advancedTabHtml),
    '#group-updates is adjacent to #group-runtime-maintenance in #tab-advanced');

  // All 10 required i18n keys
  const requiredUpdaterKeys = [
    'lbl_check_updates_auto',
    'hint_check_updates_auto',
    'btn_check_now',
    'status_checking_updates',
    'status_up_to_date',
    'status_update_available',
    'update_pill',
    'err_update_offline',
    'btn_install_now',
    'btn_install_on_close',
    'status_update_deferred_busy',
  ];

  setLanguage('es');
  for (const key of requiredUpdaterKeys) {
    const val = t(key);
    assert(Boolean(val && val !== key), `Spanish translation exists for ${key}`);
  }
  assert(t('hint_check_updates_auto').includes('GitHub Releases') && t('hint_check_updates_auto').includes('24 h'),
    'Spanish auto update hint contains privacy disclosure and 24h cadence');

  setLanguage('en');
  for (const key of requiredUpdaterKeys) {
    const val = t(key);
    assert(Boolean(val && val !== key), `English translation exists for ${key}`);
  }
  assert(t('hint_check_updates_auto').includes('GitHub Releases') && t('hint_check_updates_auto').includes('24h'),
    'English auto update hint contains privacy disclosure and 24h cadence');

  // App.js IPC and event wiring
  assert(appSource.includes("invoke('check_for_updates'"), 'check_for_updates IPC command invoked');
  assert(appSource.includes("invoke('install_update_now'"), 'install_update_now IPC command invoked');
  assert(appSource.includes("invoke('set_install_on_close'"), 'set_install_on_close IPC command invoked');
  assert(appSource.includes("invoke('get_updater_status'"), 'get_updater_status IPC command invoked');
  assert(appSource.includes("listen('update-available'"), 'update-available event listener registered');


  // 17. Honest Active Hardware & Fallback Verification
  console.log('\n--- Verifying Honest Active Hardware & Fallback ---');
  assert(runtimeSettings.hardwareStatusLabel({ is_running: true, model_size: 'small', device: 'cuda', effective_device: 'directml', active_device: 'Mic 1' })
    .includes('GPU (DirectML)'),
    'hardwareStatusLabel reports GPU (DirectML) when effective_device is directml');
  assert(runtimeSettings.hardwareStatusLabel({ is_running: true, model_size: 'small', device: 'cuda', effective_device: 'cuda', active_device: 'Mic 1' })
    .includes('GPU (CUDA)'),
    'hardwareStatusLabel reports GPU (CUDA) when effective_device is cuda');
  assert(runtimeSettings.hardwareStatusLabel({ is_running: true, model_size: 'small', device: 'cuda', effective_device: 'cpu', fallback_occurred: true, active_device: 'Mic 1' })
    .includes('CPU (Fallback)'),
    'hardwareStatusLabel reports CPU (Fallback) when fallback_occurred is true');
  assert(runtimeSettings.hardwareStatusLabel({ is_running: true, model_size: 'small', device: 'cpu', effective_device: 'cpu', fallback_occurred: false, active_device: 'Mic 1' })
    .includes('CPU'),
    'hardwareStatusLabel reports standard CPU when effective_device is cpu without fallback');

  // 18. Auto-Activation of Live Subtitle Visualizer Feed
  console.log('\n--- Verifying Live Subtitle Visualizer Auto-Activation ---');
  assert(htmlContent.includes('id="feed-live-badge"'), '#feed-live-badge element exists in index.html');
  assert(htmlContent.includes('class="badge badge-feed-idle"'), '#feed-live-badge starts in idle state');
  assert(cssContent.includes('.badge-feed-live'), '.badge-feed-live CSS class exists');
  assert(cssContent.includes('.badge-feed-idle'), '.badge-feed-idle CSS class exists');
  assert(cssContent.includes('#card-subtitles-feed.feed-active'), '#card-subtitles-feed.feed-active CSS rule exists');
  assert(appSource.includes("elements.feedLiveBadge.className = isFeedLive ? 'badge badge-feed-live' : 'badge badge-feed-idle'"),
    'app.js updates feedLiveBadge class dynamically based on live feed state');
  assert(appSource.includes("const isFeedLive = Boolean(status.is_running || status.monitor_mode || (monitorFeed && monitorFeed.socket));"),
    'Feed active state auto-activates when service is running or in monitor mode');

  // 19. Blank runtime_setup_progress text suppression
  console.log('\n--- Verifying Blank Setup Progress Suppression ---');
  assert(appSource.includes("const cleanText = (text || '').trim(); if (!cleanText) return;"),
    'Blank runtime_setup_progress text is suppressed from logger output');

  // 20. H5g: transcription engine selector, GPU status and opt-in NVIDIA runtime download
  console.log('\n--- Verifying Engine Selector and NVIDIA Runtime Opt-in ---');
  const advancedPaneH5 = htmlContent.slice(htmlContent.indexOf('id="tab-advanced"'));
  assert(/<select id="select-asr-backend"[^>]*>\s*<option value="auto"[^>]*>[\s\S]*?<option value="ort"[^>]*>[\s\S]*?<option value="ct2_cuda"[^>]*>[\s\S]*?<\/select>/.test(advancedPaneH5)
    && (advancedPaneH5.match(/<select id="select-asr-backend"[\s\S]*?<\/select>/)[0].match(/<option /g) || []).length === 3,
  'Advanced tab has the engine select with auto, ort and ct2_cuda options');
  assert(appSource.includes("elements.selectAsrBackend.value = config.asr_backend || 'auto'")
    && appSource.includes('cfg.asr_backend = elements.selectAsrBackend.value'),
  'Engine select loads from and saves to asr_backend');
  for (const key of ['lbl_asr_backend', 'hint_asr_backend', 'opt_asr_backend_auto', 'opt_asr_backend_ort', 'opt_asr_backend_ct2',
    'lbl_gpu_status', 'btn_nvidia_enable', 'btn_nvidia_dismiss', 'btn_nvidia_cancel', 'btn_nvidia_enable_link', 'btn_nvidia_restart',
    'btn_nvidia_retry', 'btn_nvidia_error_details', 'nvidia_card_title', 'nvidia_card_benefit', 'nvidia_card_size', 'nvidia_card_installing',
    'nvidia_progress_detail', 'nvidia_card_success_title', 'nvidia_card_success_next', 'nvidia_card_error_title', 'nvidia_card_error_hint', 'nvidia_card_unavailable',
    'nvidia_confirm_title', 'nvidia_confirm_message', 'nvidia_confirm_accept', 'nvidia_install_cancelled', 'nvidia_licenses_label',
    'gpu_status_adapters', 'gpu_status_none', 'gpu_status_driver', 'gpu_status_runtime_installed', 'gpu_status_runtime_missing',
    'gpu_status_missing_files', 'gpu_status_selected', 'gpu_status_active', 'gpu_reason_unknown', 'engine_using', 'engine_will_use',
    'engine_name_cuda', 'engine_name_directml', 'engine_name_cpu', 'engine_technical_details']) {
    assert((i18nContent.match(new RegExp(`\\b${key}:`, 'g')) || []).length === 2, `${key} is localized in Spanish and English`);
  }
  const reasonCodes = ['no_nvidia_gpu', 'cuda_driver_missing', 'cuda_driver_too_old', 'nvidia_runtime_not_installed',
    'nvidia_cuda_ready', 'user_selected_ort', 'ct2_preflight_failed', 'ct2_start_failed'];
  assert(typeof runtimeSettings.gpuReasonKey === 'function'
    && reasonCodes.every((code) => runtimeSettings.gpuReasonKey(code) === `gpu_reason_${code}`
      && (i18nContent.match(new RegExp(`\\bgpu_reason_${code}:`, 'g')) || []).length === 2)
    && runtimeSettings.gpuReasonKey('something_new') === 'gpu_reason_unknown'
    && runtimeSettings.gpuReasonKey(null) === 'gpu_reason_unknown',
  'GPU reason codes map to localized keys with an unknown fallback');
  const offerBase = {
    adapters: [{ vendor: 'nvidia', name: 'RTX', dedicated_vram_mb: 12000 }], cuda_driver_version: 12080,
    nvidia_runtime: { installed: false, missing: ['x'] }, license_urls: [], selected_backend: 'ort', reason: 'nvidia_runtime_not_installed',
  };
  assert(typeof runtimeSettings.nvidiaOfferVisible === 'function'
    && runtimeSettings.nvidiaOfferVisible(offerBase, false) === true
    && runtimeSettings.nvidiaOfferVisible(offerBase, true) === false
    && runtimeSettings.nvidiaOfferVisible({ ...offerBase, cuda_driver_version: 12070 }, false) === false
    && runtimeSettings.nvidiaOfferVisible({ ...offerBase, cuda_driver_version: null }, false) === false
    && runtimeSettings.nvidiaOfferVisible({ ...offerBase, adapters: [{ vendor: 'amd', name: 'RX', dedicated_vram_mb: 8000 }] }, false) === false
    && runtimeSettings.nvidiaOfferVisible({ ...offerBase, nvidia_runtime: { installed: true, missing: [] } }, false) === false
    && runtimeSettings.nvidiaOfferVisible(null, false) === false,
  'NVIDIA offer shows only with an NVIDIA GPU, driver >= 12080, runtime missing and not dismissed');
  assert(htmlContent.includes('id="card-nvidia-runtime"') && htmlContent.includes('id="btn-nvidia-enable"')
    && htmlContent.includes('id="btn-nvidia-cancel"') && htmlContent.includes('id="nvidia-install-meter"')
    && htmlContent.includes('id="btn-nvidia-dismiss"') && htmlContent.includes('id="gpu-status-text"')
    && htmlContent.includes('id="modal-links"'),
  'NVIDIA offer card, progress, cancel, dismiss, GPU status and modal links exist');
  assert(jsContent.includes("listen('nvidia_runtime_progress'"), 'App listens for nvidia_runtime_progress');
  assert((jsContent.match(/invoke\('install_nvidia_runtime'\)/g) || []).length === 1
    && /const accepted = await showConfirmModal\(\{[\s\S]*?links:[\s\S]*?\}\);\s*if \(!accepted\) return;[\s\S]*?invoke\('install_nvidia_runtime'\)/.test(jsContent),
  'NVIDIA install is invoked once and only after the license confirmation is accepted');
  assert(/btnNvidiaCancel\.addEventListener\('click'[\s\S]*?invoke\('cancel_nvidia_runtime_install'\)/.test(jsContent),
    'Cancel button calls cancel_nvidia_runtime_install');
  assert(/try \{[^}]*localStorage\.setItem\(NVIDIA_DISMISS_KEY/.test(jsContent) && /try \{[^}]*localStorage\.getItem\(NVIDIA_DISMISS_KEY/.test(jsContent),
    'Banner dismissal is stored per viewer in guarded localStorage');
  assert(jsContent.includes("invoke('get_gpu_backend_status')") && jsContent.includes('active_backend'),
    'GPU status is loaded from the backend and shows the active backend');
  assert(jsContent.includes("invoke('open_nvidia_license_url', { url })"),
    'License links open through the native allow-listed command');
  assert(/pub async fn open_nvidia_license_url\(url: String\)/.test(nativeCommands)
    && nativeCommands.includes('license_urls().contains')
    && nativeLib.includes('commands::open_nvidia_license_url'),
  'Native license opener only accepts URLs from the pinned license list and is registered');
  assert(dtsContent.includes('interface GpuBackendStatus') && dtsContent.includes('license_urls: string[]'),
    'types.d.ts declares GpuBackendStatus');

  // NVIDIA license links: short localized labels, full URL in a tooltip, accent-themed chips
  assert((i18nContent.match(/\bnvidia_license_cublas: 'Licencia de cuBLAS'/g) || []).length === 1
    && (i18nContent.match(/\bnvidia_license_cublas: 'cuBLAS license'/g) || []).length === 1
    && (i18nContent.match(/\bnvidia_license_cudnn: 'Licencia de cuDNN'/g) || []).length === 1
    && (i18nContent.match(/\bnvidia_license_cudnn: 'cuDNN license'/g) || []).length === 1,
  'NVIDIA license link labels are localized in Spanish and English');
  assert(/function licenseLinkLabel\(url\) \{[\s\S]*?url\.includes\('libcublas'\)[\s\S]*?t\('nvidia_license_cublas'\)[\s\S]*?url\.includes\('cudnn'\)[\s\S]*?t\('nvidia_license_cudnn'\)[\s\S]*?new URL\(url\)\.hostname/.test(jsContent),
  'License label is derived from the URL (cuBLAS, cuDNN) with a hostname fallback');
  assert(jsContent.includes('a.textContent = licenseLinkLabel(url);') && !jsContent.includes('a.textContent = url;'),
  'License links show a short label instead of the raw URL as link text');
  assert(jsContent.includes('a.title = url;') && jsContent.includes("a.className = 'modal-license-link';"),
  'License links expose the full URL as a title tooltip and use the modal-license-link class');
  assert(htmlContent.includes('<div id="modal-links" class="modal-links" hidden></div>'),
  'Modal links container uses the dedicated modal-links class');
  assert(/\.modal-links\s*\{[^}]*flex-wrap:\s*wrap/.test(cssContent),
  'Modal links container wraps its chips instead of overflowing');
  assert(/\.modal-license-link\s*\{[^}]*color:\s*var\(--accent-primary\)[^}]*overflow-wrap:\s*anywhere/s.test(cssContent)
    && /\.modal-license-link::after\s*\{[^}]*content:\s*"\\2197"/s.test(cssContent),
  'License link chips use the accent color, wrap long text, and show an external-link arrow');
  assert(/\.modal-license-link:hover\s*\{[^}]*text-decoration:\s*underline/s.test(cssContent)
    && /\.modal-license-link:focus-visible[\s\S]*?\{[^}]*outline:/.test(cssContent),
  'License link chips underline on hover and show a visible focus ring');

  // 21. Home-screen NVIDIA card, Advanced engine summary and plain-language engine copy
  console.log('\n--- Verifying Home NVIDIA Card and Engine Summary ---');
  const homeCardIds = ['card-nvidia-runtime', 'nvidia-offer', 'nvidia-gpu-name', 'btn-nvidia-enable', 'btn-nvidia-dismiss',
    'nvidia-install-progress', 'nvidia-install-meter', 'nvidia-install-detail', 'btn-nvidia-cancel', 'nvidia-result',
    'nvidia-result-title', 'nvidia-result-text', 'btn-nvidia-restart', 'btn-nvidia-retry', 'nvidia-error-details', 'nvidia-error-text'];
  assert(homeCardIds.every((id) => setupPane.includes(`id="${id}"`)),
    'The NVIDIA card with its offer, progress, result and error controls lives on the home tab');
  assert(setupPane.indexOf('id="card-nvidia-runtime"') < setupPane.indexOf('id="select-audio-device"'),
    'The NVIDIA card is the first element of the home tab');
  assert(!advancedPane.includes('card-nvidia-runtime') && !advancedPane.includes('id="btn-nvidia-enable"')
    && !advancedPane.includes('nvidia-install-') && (htmlContent.match(/id="nvidia-install-meter"/g) || []).length === 1,
  'The old Advanced NVIDIA card is removed and one install state is rendered by the home card');
  const engineOrder = ['engine-status-pill', 'btn-nvidia-advanced', 'select-asr-backend', 'engine-technical-details'];
  assert(engineOrder.every((id, index) => advancedPane.includes(`id="${id}"`)
    && (index === 0 || advancedPane.indexOf(`id="${engineOrder[index - 1]}"`) < advancedPane.indexOf(`id="${id}"`))),
  'Advanced engine section reads top-down: status summary, engine select, then technical details');
  assert(/<details id="engine-technical-details"[^>]*>\s*<summary[^>]*data-i18n="engine_technical_details"[^>]*>[\s\S]*?id="gpu-status-text"[\s\S]*?<\/details>/.test(advancedPane),
    'Raw GPU, driver, runtime and engine data sit inside a collapsed technical details element');
  assert(/function renderNvidiaCard\(\) \{[\s\S]*?nvidiaOfferVisible\(gpuStatus, nvidiaOfferDismissed\(\)\)/.test(jsContent)
    && /function renderEngineSummary\(\) \{[\s\S]*?engineKind\(gpuStatus, live\)/.test(jsContent),
  'The home card is gated by nvidiaOfferVisible and the Advanced summary reads the shared engine kind');
  assert(/btnNvidiaAdvanced\.addEventListener\('click', \(\) => \{\s*elements\.tabSetupTab\.click\(\);\s*startNvidiaInstall\(\);/.test(jsContent)
    && /btnNvidiaRestart\.addEventListener\('click', restartServiceAfterNvidia\)/.test(jsContent)
    && /async function restartServiceAfterNvidia\(\) \{[\s\S]*?invoke\('restart_service'\)/.test(jsContent),
  'The Advanced link opens the home card and runs the same install; the success restart reuses restart_service');
  assert(nativeCommands.includes('downloaded_bytes') && nativeCommands.includes('total_bytes')
    && jsContent.includes('p.downloaded_bytes') && jsContent.includes('p.total_bytes'),
  'NVIDIA install progress carries download byte counts for the MB readout');
  assert(!/\b(nvidia_offer_text|nvidia_install_done|nvidia_install_failed):/.test(i18nContent),
    'Copy replaced by the home card is removed from both languages');

  const plainReasonsEs = {
    gpu_reason_no_nvidia_gpu: 'No se detectó una tarjeta NVIDIA; se usa el modo compatible.',
    gpu_reason_nvidia_runtime_not_installed: 'Falta activar la aceleración NVIDIA.',
    gpu_reason_cuda_driver_too_old: 'Actualiza el driver de NVIDIA para usar la aceleración.',
    gpu_reason_ct2_start_failed: 'La aceleración NVIDIA no pudo iniciar; se usó el modo compatible.',
  };
  const plainReasonsEn = {
    gpu_reason_no_nvidia_gpu: 'No NVIDIA graphics card was detected; the compatible mode is used.',
    gpu_reason_nvidia_runtime_not_installed: 'NVIDIA acceleration is not turned on yet.',
    gpu_reason_cuda_driver_too_old: 'Update the NVIDIA driver to use acceleration.',
    gpu_reason_ct2_start_failed: 'NVIDIA acceleration could not start; the compatible mode was used.',
  };
  setLanguage('es');
  const esReasonsPlain = Object.entries(plainReasonsEs).every(([key, text]) => t(key) === text);
  setLanguage('en');
  const enReasonsPlain = Object.entries(plainReasonsEn).every(([key, text]) => t(key) === text);
  setLanguage('es');
  assert(esReasonsPlain && enReasonsPlain, 'Engine reasons read in plain language in Spanish and English');

  setLanguage('es');
  const esEngineLabels = [t('opt_asr_backend_auto'), t('opt_asr_backend_ort'), t('opt_asr_backend_ct2')];
  setLanguage('en');
  const enEngineLabels = [t('opt_asr_backend_auto'), t('opt_asr_backend_ort'), t('opt_asr_backend_ct2')];
  setLanguage('es');
  assert(JSON.stringify(esEngineLabels) === JSON.stringify(['Automático (recomendado)', 'Compatible (DirectML / CPU)', 'NVIDIA CUDA'])
    && JSON.stringify(enEngineLabels) === JSON.stringify(['Automatic (recommended)', 'Compatible (DirectML / CPU)', 'NVIDIA CUDA']),
  'Engine options use friendly labels in Spanish and English');

  const rtxGpu = { adapters: [{ vendor: 'nvidia', name: 'RTX', dedicated_vram_mb: 12000 }], cuda_driver_version: 13040,
    nvidia_runtime: { installed: true, missing: [] }, license_urls: [], selected_backend: 'ort', reason: 'user_selected_ort' };
  assert(typeof runtimeSettings.engineKind === 'function'
    && runtimeSettings.engineKind({ ...rtxGpu, selected_backend: 'ct2_cuda' }, null) === 'cuda'
    && runtimeSettings.engineKind(rtxGpu, null) === 'directml'
    && runtimeSettings.engineKind({ ...rtxGpu, adapters: [] }, null) === 'cpu'
    && runtimeSettings.engineKind(rtxGpu, { is_running: false, active_backend: 'ct2_cuda' }) === 'directml'
    && runtimeSettings.engineKind(null, { is_running: true, active_backend: 'ct2_cuda' }) === 'cuda'
    && runtimeSettings.engineKind(null, { is_running: true, active_backend: 'ort', effective_device: 'directml' }) === 'directml'
    && runtimeSettings.engineKind(null, { is_running: true, active_backend: 'ort', effective_device: 'cpu' }) === 'cpu'
    && runtimeSettings.engineKind(null, null) === null,
  'Engine kind follows the active engine while running and the selected engine otherwise');

  // Home card and Advanced summary through the real app.js (vm harness)
  const rtxStatus = { adapters: [{ vendor: 'nvidia', name: 'NVIDIA GeForce RTX 3060', dedicated_vram_mb: 12115 }], cuda_driver_version: 13040,
    nvidia_runtime: { installed: false, missing: ['cublas64_12.dll'] }, license_urls: [], selected_backend: 'ort', reason: 'nvidia_runtime_not_installed' };
  const loadNvidiaHarness = (options) => { setLanguage('es'); return loadAppHarness(styleBaseConfig, options); };
  {
    const h = await loadNvidiaHarness({ gpuStatus: rtxStatus, deriveSelected: true });
    assert(h.el('card-nvidia-runtime').hidden === false && h.el('nvidia-offer').hidden === false
      && h.el('nvidia-install-progress').hidden === true && h.el('nvidia-result').hidden === true
      && h.el('nvidia-gpu-name').textContent === 'NVIDIA GeForce RTX 3060',
    'The home card offers activation with the detected GPU name');
    assert(h.el('engine-status-text').textContent === `${t('engine_will_use')}${t('engine_name_directml')}`
      && h.el('engine-status-reason').textContent === t('gpu_reason_nvidia_runtime_not_installed')
      && h.el('btn-nvidia-advanced').hidden === false,
    'Advanced summary names the engine that will be used, the plain reason and the activation link');

    h.el('btn-nvidia-enable').click();
    await h.settle();
    assert(h.el('modal-icon').innerHTML.includes('lucide-download'), 'NVIDIA install confirmation uses the download icon');
    h.el('modal-btn-confirm').click();
    await h.settle();
    assert(h.backend.calls.some((call) => call.cmd === 'install_nvidia_runtime')
      && h.el('nvidia-result-title').textContent === t('nvidia_card_success_title')
      && h.el('nvidia-result-text').textContent === t('nvidia_card_success_next') && h.el('btn-nvidia-restart').hidden === true,
    'A finished install on a stopped service says it will be used next time, without a restart button');
  }
  {
    const running = await loadNvidiaHarness({ gpuStatus: rtxStatus, serviceRunning: true, deriveSelected: true });
    running.el('btn-nvidia-enable').click();
    await running.settle();
    running.el('modal-btn-confirm').click();
    await running.settle();
    assert(running.el('nvidia-result-title').textContent === t('nvidia_card_success_title') && running.el('btn-nvidia-restart').hidden === false,
    'A finished install on a running service offers the restart button');
    running.el('btn-nvidia-restart').click();
    await running.settle();
    assert(running.backend.calls.some((call) => call.cmd === 'restart_service') && running.el('card-nvidia-runtime').hidden === true,
    'Restart reuses restart_service and clears the finished card');
  }
  {
    const failing = await loadNvidiaHarness({ gpuStatus: rtxStatus, installError: 'connection reset' });
    failing.el('btn-nvidia-enable').click();
    await failing.settle();
    failing.el('modal-btn-confirm').click();
    await failing.settle();
    assert(failing.el('nvidia-result-title').textContent === t('nvidia_card_error_title') && failing.el('btn-nvidia-retry').hidden === false
      && failing.el('btn-nvidia-restart').hidden === true && failing.el('nvidia-error-details').hidden === false
      && failing.el('nvidia-error-text').textContent.includes('connection reset'),
    'A failed install shows a friendly message and retry, with the technical error under details');
  }
  {
    const notYet = await loadNvidiaHarness({ gpuStatus: rtxStatus, installError: 'unexpected HTTP status 404' });
    notYet.el('btn-nvidia-enable').click();
    await notYet.settle();
    notYet.el('modal-btn-confirm').click();
    await notYet.settle();
    assert(notYet.el('nvidia-result-text').textContent === t('nvidia_card_unavailable') && notYet.el('nvidia-error-text').textContent.includes('404'),
    'A 404 from the download shows that acceleration is not available for this version yet');
  }
  {
    const cuda = await loadNvidiaHarness({ gpuStatus: { ...rtxStatus, nvidia_runtime: { installed: true, missing: [] }, selected_backend: 'ct2_cuda', reason: 'nvidia_cuda_ready' },
      serviceRunning: true, activeBackend: 'ct2_cuda', backendReason: 'nvidia_cuda_ready' });
    assert(cuda.el('engine-status-text').textContent === `${t('engine_using')}${t('engine_name_cuda')}`
      && cuda.el('engine-status-pill').className === 'pill state-listening'
      && cuda.el('engine-status-reason').textContent === t('gpu_reason_nvidia_cuda_ready')
      && cuda.el('btn-nvidia-advanced').hidden === true && cuda.el('card-nvidia-runtime').hidden === true,
    'A running NVIDIA engine is named in plain language and hides the home card and activation link');
    assert(cuda.el('gpu-status-text').textContent.includes('NVIDIA GeForce RTX 3060 (12115 MB VRAM)')
      && cuda.el('gpu-status-text').textContent.includes('Controlador CUDA: 13.4'),
    'Technical details keep the GPU name, VRAM and CUDA driver version');
  }
  {
    const dismissed = await loadNvidiaHarness({ gpuStatus: rtxStatus });
    dismissed.el('btn-nvidia-dismiss').click();
    await dismissed.settle();
    assert(dismissed.el('card-nvidia-runtime').hidden === true && dismissed.el('btn-nvidia-advanced').hidden === false,
    '"Ahora no" hides the home card while the Advanced activation link stays available');
    localStorage.removeItem('liveaudio.nvidiaOfferDismissed');
  }
  {
    // Live-state card: derived from the running engine, not only from the in-memory install state.
    const installed = { ...rtxStatus, nvidia_runtime: { installed: true, missing: [] }, selected_backend: 'ct2_cuda', reason: 'nvidia_cuda_ready' };
    const view = runtimeSettings.nvidiaCardView;
    assert(typeof view === 'function'
      && view(installed, { is_running: true, active_backend: 'ort' }, 'idle') === 'success'
      && view(installed, { is_running: true, active_backend: 'ct2_cuda' }, 'idle') === 'idle'
      && view(installed, { is_running: true, active_backend: 'ct2_cuda' }, 'success') === 'idle'
      && view(installed, { is_running: false }, 'idle') === 'idle'
      && view(installed, { is_running: false }, 'success') === 'success'
      && view({ ...installed, selected_backend: 'ort' }, { is_running: true, active_backend: 'ort' }, 'idle') === 'idle'
      && view({ ...installed, selected_backend: 'ort' }, { is_running: false }, 'success') === 'idle'
      && view(installed, { is_running: true, active_backend: 'ort', backend_reason: 'ct2_start_failed' }, 'idle') === 'idle'
      && view(installed, { is_running: true, active_backend: 'ort' }, 'installing') === 'installing'
      && view(installed, { is_running: true, active_backend: 'ort' }, 'error') === 'error',
    'NVIDIA card state follows the live engine: restart only when running on ORT with NVIDIA ready');

    const stale = await loadNvidiaHarness({ gpuStatus: installed, serviceRunning: true, activeBackend: 'ort', backendReason: 'user_selected_ort' });
    assert(stale.el('card-nvidia-runtime').hidden === false && stale.el('btn-nvidia-restart').hidden === false
      && stale.el('nvidia-result-text').textContent === t('nvidia_card_success_restart'),
    'Installed runtime while the service runs on ORT shows one restart action on a fresh app start');
    const idleStopped = await loadNvidiaHarness({ gpuStatus: installed, serviceRunning: false });
    assert(idleStopped.el('card-nvidia-runtime').hidden === true,
      'Installed runtime with the service stopped shows no card (the engine summary says NVIDIA will be used)');

    const fresh = await loadNvidiaHarness({ gpuStatus: rtxStatus, deriveSelected: true });
    fresh.el('btn-nvidia-enable').click();
    await fresh.settle();
    fresh.el('modal-btn-confirm').click();
    await fresh.settle();
    const saves = fresh.backend.calls.filter((call) => call.cmd === 'save_config');
    assert(saves.length === 1 && saves[0].args.config.device === 'cuda' && saves[0].args.config.asr_backend === 'auto'
      && fresh.el('nvidia-result-text').textContent === t('nvidia_card_success_next'),
    'A finished install sets device cuda and asr_backend auto so the next start uses NVIDIA');

    const ortConfig = { ...styleBaseConfig, asr_backend: 'ort' };
    const explicit = await loadAppHarness(ortConfig, { gpuStatus: rtxStatus, deriveSelected: true });
    explicit.el('btn-nvidia-enable').click();
    await explicit.settle();
    explicit.el('modal-btn-confirm').click();
    await explicit.settle();
    assert(!explicit.backend.calls.some((call) => call.cmd === 'save_config') && explicit.el('card-nvidia-runtime').hidden === true,
      'An explicit asr_backend ort is never overridden and the card shows nothing after install');
    assert(runtimeSettings.nvidiaInstallConfig({ asr_backend: 'ort', device: 'cpu' }) === null
      && runtimeSettings.nvidiaInstallConfig({ asr_backend: 'ct2_cuda', device: 'cpu' }).asr_backend === 'ct2_cuda'
      && runtimeSettings.nvidiaInstallConfig({ asr_backend: 'auto', device: 'cpu' }).device === 'cuda',
    'Install config keeps ct2_cuda, upgrades others to auto, and skips an explicit ort');
    assert(/loadObsOverlayInfo\(\);\s*refreshGpuStatus\(\);\s*updateStatusUI\(currentStatus\);/.test(jsContent),
      'Saving settings refreshes the engine status so the pre-start text follows device and backend');
  }

  // About / support / usage statistics
  {
    const aboutKeys = ['btn_about', 'about_title', 'about_close', 'about_stats_title', 'about_stat_time', 'about_stat_subtitles',
      'about_stat_sessions', 'about_since', 'about_stats_local', 'about_stats_empty', 'about_lt_minute', 'about_support_title',
      'about_report', 'about_report_copied', 'about_star', 'about_sponsor', 'about_support_note', 'toast_support_open_failed'];
    for (const lang of ['es', 'en']) {
      setLanguage(lang);
      for (const key of aboutKeys) assert(Boolean(t(key) && t(key) !== key), `About key ${key} exists in ${lang}`);
    }
    setLanguage('es');
    const aboutHtml = htmlContent.match(/<div id="about-modal"[\s\S]*?<!-- \/about-modal -->/)?.[0] || '';
    const headerHtml = htmlContent.match(/<header[\s\S]*?<\/header>/)?.[0] || '';
    assert(/<button[^>]*id="btn-about"[^>]*class="brand-badge"[^>]*data-i18n-aria-label="btn_about"[\s\S]*?data-i18n="app_version"/.test(headerHtml)
      && !headerHtml.includes('author-profile-link') && !headerHtml.includes('lucide-info'),
    'Header version badge is the localized About button; no author link or info icon');
    assert((aboutHtml.match(/data-i18n="app_version"/g) || []).length === 1 && aboutHtml.includes('id="author-profile-link"'),
    'About shows the version once and hosts the author profile link');
    assert(/role="dialog"/.test(aboutHtml) && aboutHtml.includes('aria-modal="true"') && aboutHtml.includes('aria-labelledby="about-title"')
      && ['about-stat-time', 'about-stat-subtitles', 'about-stat-sessions', 'about-since', 'about-empty', 'btn-about-close',
        'btn-about-report', 'btn-about-star', 'btn-about-sponsor'].every((id) => aboutHtml.includes(`id="${id}"`)),
    'About modal is an accessible dialog with stats, support actions and a close button');
    assert(['lucide-bug', 'lucide-star', 'lucide-heart'].every((name) => aboutHtml.includes(name))
,
    'About uses vendored Lucide icons');
    assert(!/\p{Extended_Pictographic}/u.test(htmlContent) && !/\p{Extended_Pictographic}/u.test(jsContent),
    'index.html and app.js contain no emojis (icons, toasts, modal copy and logs)');
    assert(/THIRD_PARTY/.test('THIRD_PARTY') && fs.readFileSync(path.join(__dirname, '..', 'THIRD_PARTY_NOTICES.md'), 'utf8').includes('Lucide'),
      'Lucide ISC notice is listed in THIRD_PARTY_NOTICES.md');
    assert(/pub async fn open_support_link\(id: String\)/.test(nativeCommands) && /pub async fn get_usage_stats\(/.test(nativeCommands)
      && /pub async fn get_support_environment\(/.test(nativeCommands)
      && ['open_support_link', 'get_usage_stats', 'get_support_environment'].every((name) => nativeLib.includes(`commands::${name}`)),
    'Support commands are registered and take no URL from the frontend');
    assert(!/invoke\('open_support_link',\s*\{\s*url/.test(jsContent), 'Support links are requested by id, never by URL');

    const h = await loadAppHarness(styleBaseConfig, { usageStats: { audio_seconds: 3720, subtitles: 1234, sessions: 7, first_used: 1700000000 } });
    h.el('btn-about').click();
    await h.settle();
    assert(h.backend.calls.some((call) => call.cmd === 'get_usage_stats') && h.el('about-modal').hidden === false,
      'The About button loads the stats and shows the modal');
    assert(h.el('about-stat-time').textContent === '1 h 2 min' && h.el('about-stat-subtitles').textContent === '1234'
      && h.el('about-stat-sessions').textContent === '7' && h.el('about-empty').hidden === true,
    'Stats are formatted as hours/minutes and plain counts');
    h.el('btn-about-close').click();
    assert(h.el('about-modal').hidden === true, 'The close button hides the About modal');
    h.el('btn-about-star').click();
    h.el('btn-about-sponsor').click();
    h.el('btn-about-report').click();
    await h.settle();
    const opened = h.backend.calls.filter((call) => call.cmd === 'open_support_link').map((call) => call.args.id);
    assert(JSON.stringify(opened) === JSON.stringify(['repo', 'sponsor', 'issues'])
      && h.backend.calls.some((call) => call.cmd === 'get_support_environment'),
    'Support buttons open fixed ids; the report button also reads the safe environment block');

    const empty = await loadAppHarness(styleBaseConfig);
    empty.el('btn-about').click();
    await empty.settle();
    assert(empty.el('about-empty').hidden === false, 'All-zero stats show the empty state');
  }

  {
    assert(/<span id="modal-icon" class="modal-icon"><svg[^>]*lucide-triangle-alert/.test(htmlContent)
      && ['lucide-triangle-alert', 'lucide-refresh-cw', 'lucide-download'].every((name) => jsContent.includes(name))
      && jsContent.includes("icon: 'refresh'") && jsContent.includes("icon: 'download'"),
    'Confirm modal icons are whitelisted Lucide SVG keys with a warning placeholder');
    const modal = await loadAppHarness(styleBaseConfig);
    const iconAfter = (options) => {
      modal.app.showConfirmModal?.({ message: 'Confirm?', ...options });
      const markup = modal.el('modal-icon').innerHTML;
      modal.el('modal-btn-cancel').click();
      return markup;
    };
    assert(iconAfter({}).includes('lucide-triangle-alert') && iconAfter({ icon: 'refresh' }).includes('lucide-refresh-cw')
      && iconAfter({ icon: 'download' }).includes('lucide-download'),
    'Modal icon keys render their Lucide SVGs and the default key is the warning icon');
    assert(iconAfter({ icon: 'toString' }).includes('lucide-triangle-alert')
      && iconAfter({ icon: '<img src=x onerror=alert(1)>' }).includes('lucide-triangle-alert')
      && !iconAfter({ icon: '<img src=x>' }).includes('<img'),
    'Unknown modal icon keys fall back to the warning SVG and caller strings never reach the markup');
    assert(!/\p{Extended_Pictographic}/u.test(iconAfter({ icon: 'refresh' })), 'Modal icon markup contains no emoji');
  }

  {
    const logSources = ['desktop/src-tauri/src/commands.rs', 'desktop/src-tauri/src/logging.rs',
      'crates/liveaudio-cli/src/main.rs', 'assets/subtitulos_obs.html'];
    // The liveaudio-cli unicode path probe keeps a non-ASCII directory name on purpose.
    const emojiLines = logSources.flatMap((file) => fs.readFileSync(path.join(__dirname, '..', file), 'utf8').split(/\r?\n/)
      .filter((line) => /\p{Extended_Pictographic}/u.test(line) && !line.includes('liveaudio doctor accénts')));
    assert(emojiLines.length === 0, 'Tracing, daemon and OBS overlay strings contain no emojis');
  }

  // Restore Spanish as default
  setLanguage('es');

  console.log(`\nResults: ${passed} passed, ${failed} failed.`);
  if (failed > 0) {
    process.exit(1);
  }
}

await runTests();
