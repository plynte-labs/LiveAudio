// SPDX-License-Identifier: MIT

/**
 * Frontend automated verification test suite for LiveAudio Desktop (Tauri 2).
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { bindProfileSelection, bindTabNavigation, persistAndApplyRuntimeConfig, runtimeSettingsChanged } from './src/runtime-settings.js';
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
  assert(htmlContent.includes('id="slider-max-chunk"') && htmlContent.includes('max="60.0"'), 'Max chunk duration slider supports up to 60.0s');
  assert(htmlContent.includes('v1.5.1'), 'Version 1.5.1 badge rendered in index.html');
  const authorLink = htmlContent.match(/<a\b[^>]*id="author-profile-link"[^>]*>([\s\S]*?)<\/a>/i);
  assert(authorLink && authorLink[0].includes('data-i18n="author_credit"'),
    'Author credit is a semantic localized anchor');
  assert(authorLink && /href="https:\/\/github\.com\/franguh"/.test(authorLink[0])
    && /target="_blank"/.test(authorLink[0])
    && /rel="noopener noreferrer"/.test(authorLink[0]),
  'Author link targets the fixed external profile safely');
  assert(authorLink && /<span class="brand-badge" data-i18n="app_version">[\s\S]*?<\/span>\s*<a\b[^>]*id="author-profile-link"/.test(htmlContent),
    'Author credit appears immediately after the version');

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

  requiredStyles.forEach((style) => {
    assert(cssContent.includes(`.${style}`), `CSS includes class .${style}`);
  });

  // Verify key animations
  assert(cssContent.includes('@keyframes popIn'), 'Karaoke popIn animation exists');
  assert(cssContent.includes('@keyframes rgbFadeIn'), 'RGB rgbFadeIn animation exists');
  assert(cssContent.includes('@keyframes typeIn'), 'Typewriter typeIn animation exists');
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
  assert((i18nContent.match(/author_credit: 'by Franguh'/g) || []).length === 2,
    'Author credit remains exactly by Franguh in both language settings');
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
  assert(subtitlePane.includes('select-subtitle-style') && !subtitlePane.includes('select-backlog-policy'),
    'Subtitles focuses on presentation settings');
  assert(subtitlePane.includes('slider-ribbon-lines') && subtitlePane.includes('val-ribbon-lines'),
    'Subtitles retains the configured maximum Ribbon line control');
  assert(advancedPane.includes('select-compute-device') && advancedPane.includes('select-model-size')
    && advancedPane.includes('slider-vad-threshold') && advancedPane.includes('select-backlog-policy')
    && advancedPane.includes('group-prompt') && advancedPane.includes('group-blacklist'),
    'Advanced contains model, compute, VAD, backlog and language filters');
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
  for (const key of ['lbl_rnnoise_enabled', 'hint_rnnoise_enabled', 'lbl_acoustic_events_enabled', 'hint_acoustic_events_enabled']) {
    assert((i18nContent.match(new RegExp(`${key}:`, 'g')) || []).length === 2,
      `${key} is localized in Spanish and English`);
  }
  assert(/hint_acoustic_events_enabled:[\s\S]*?OpenCohost[\s\S]*?metadata/.test(i18nContent)
    && /hint_rnnoise_enabled:[\s\S]*?RNNoise[\s\S]*?VAD/.test(i18nContent)
    && !i18nContent.includes('not active yet')
    && !i18nContent.includes('integration is complete'),
    'help copy describes active RNNoise and metadata-only events without interim wording');

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
    id: 'balanced', device: 'cuda', model_size: 'small', silence_timeout: 0.8,
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

  // 6. Verify version 1.5.1 consistency across workspace and manifests
  const rootCargo = fs.readFileSync(path.join(__dirname, '..', 'Cargo.toml'), 'utf8');
  assert(rootCargo.includes('version = "1.5.1"'), 'Root Cargo.toml has version 1.5.1');
  const tauriConf = JSON.parse(fs.readFileSync(path.join(__dirname, 'src-tauri', 'tauri.conf.json'), 'utf8'));
  assert(tauriConf.version === '1.5.1', 'tauri.conf.json has version 1.5.1');
  assert(tauriConf.bundle.category === 'Music', 'Tauri bundle category uses a supported audio-related category');
  const expectedResourceMap = {
    '../../THIRD_PARTY_NOTICES.md': 'THIRD_PARTY_NOTICES.md',
    '../../liveaudio/*.py': 'liveaudio',
    '../../liveaudio/assets/*': 'liveaudio/assets',
    '../../liveaudio/core/*.py': 'liveaudio/core',
    '../../liveaudio/service/*.py': 'liveaudio/service',
    '../../liveaudio/utils/*.py': 'liveaudio/utils',
  };
  assert(JSON.stringify(tauriConf.bundle.resources) === JSON.stringify(expectedResourceMap),
    'Tauri maps the third-party notice and direct runtime files');
  const platformConf = (name) => JSON.parse(fs.readFileSync(path.join(__dirname, 'src-tauri', name), 'utf8'));
  assert(platformConf('tauri.windows.conf.json').bundle.resources['../../packaging/vendor/uv.exe'] === 'packaging/vendor/uv.exe',
    'Windows bundle maps the vendored uv.exe');
  assert(platformConf('tauri.linux.conf.json').bundle.resources['../../packaging/vendor/uv'] === 'packaging/vendor/uv',
    'Linux bundle maps the vendored uv binary');
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
    source === '../../liveaudio/service/*.py' && target === 'liveaudio/service'),
  'ASR worker is mapped to the resolver-supported liveaudio/service path');
  assert(Object.keys(tauriConf.bundle.resources).every(source =>
    !source.includes('**') && !source.includes('__pycache__') && !source.endsWith('.pyc')),
  'Tauri resource globs do not recursively include Python bytecode caches');
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
  assert(hooksContent.includes('$APPDATA\\LiveAudio') && !hooksContent.includes('RMDir /r "$APPDATA\\LiveAudio"'),
    'hooks.nsh protects and preserves user configuration and data in %APPDATA%\\LiveAudio');
  assert(hooksContent.includes('ExecWait'), 'hooks.nsh executes legacy uninstaller silently');
  assert(hooksContent.includes('/VERYSILENT') && hooksContent.includes('/S'),
    'hooks.nsh supports both InnoSetup and NSIS silent uninstaller switches');
  assert(hooksContent.includes('liveaudio-desktop.exe'),
    'hooks.nsh identifies modern Rust desktop client to prevent false-positive uninstallation');

  assert(t('app_version').includes('v1.5.1'), 'i18n app_version includes v1.5.1');

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
  assert(appSource.includes('!currentStatus.is_running || currentStatus.monitor_mode'),
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
  assert(appSource.includes('savedConfig = structuredClone(newConfig);'),
    'savedConfig baseline is synchronized on blacklist and prompt additions');
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

  // Restore Spanish as default
  setLanguage('es');

  console.log(`\nResults: ${passed} passed, ${failed} failed.`);
  if (failed > 0) {
    process.exit(1);
  }
}

await runTests();
