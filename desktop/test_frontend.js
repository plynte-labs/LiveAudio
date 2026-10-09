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
  assert(htmlContent.includes('v1.5.0'), 'Version 1.5.0 badge rendered in index.html');
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
  assert(htmlContent.includes('id="select-profile"')
    && htmlContent.includes('id="slider-catchup-interval"')
    && htmlContent.includes('id="btn-open-session-folder"')
    && htmlContent.includes('id="check-continuous"')
    && htmlContent.includes('id="check-save-vtt"'),
    'profile, subtitle timing and session file controls are present');

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

  const baseline = { model_size: 'small', device: 'cpu', ws_port: 8765, subtitle_style: 'default' };
  assert(!runtimeSettingsChanged(baseline, { ...baseline, subtitle_style: 'neon' }),
    'style-only change does not request a service restart');
  assert(runtimeSettingsChanged(baseline, { ...baseline, ws_port: 9000 }),
    'WebSocket port change requires a confirmed service restart');
  assert(runtimeSettingsChanged(baseline, { ...baseline, model_size: 'turbo' }),
    'ASR model change requires a confirmed service restart');
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
  let profileDraft = { ...savedBaseline, audio_device: { id: 'usb', name: 'USB mic' }, subtitle_style: 'neon' };
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

  setLanguage('es');
  assert(getLanguage() === 'es', 'language setter activates Spanish');
  assert(t('tab_setup') === 'Inicio' && t('tab_files') === 'Archivos', 'Spanish settings navigation labels exist');
  assert(t('btn_start') === 'Iniciar Transcripción', 'Spanish translation for btn_start');
  assert(t('status_stopped') === 'Detenido', 'Spanish translation for status_stopped');
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

  // 6. Verify version 1.5.0 consistency across workspace and manifests
  const rootCargo = fs.readFileSync(path.join(__dirname, '..', 'Cargo.toml'), 'utf8');
  assert(rootCargo.includes('version = "1.5.0"'), 'Root Cargo.toml has version 1.5.0');
  const tauriConf = JSON.parse(fs.readFileSync(path.join(__dirname, 'src-tauri', 'tauri.conf.json'), 'utf8'));
  assert(tauriConf.version === '1.5.0', 'tauri.conf.json has version 1.5.0');
  assert(tauriConf.bundle.category === 'Music', 'Tauri bundle category uses a supported audio-related category');
  const expectedResourceMap = {
    '../../liveaudio/*.py': 'liveaudio',
    '../../liveaudio/assets/*': 'liveaudio/assets',
    '../../liveaudio/core/*.py': 'liveaudio/core',
    '../../liveaudio/service/*.py': 'liveaudio/service',
    '../../liveaudio/utils/*.py': 'liveaudio/utils',
  };
  assert(JSON.stringify(tauriConf.bundle.resources) === JSON.stringify(expectedResourceMap),
    'Tauri maps only direct runtime files to stable liveaudio resource paths');
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
  assert(t('app_version').includes('v1.5.0'), 'i18n app_version includes v1.5.0');

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

  // Restore Spanish as default
  setLanguage('es');

  console.log(`\nResults: ${passed} passed, ${failed} failed.`);
  if (failed > 0) {
    process.exit(1);
  }
}

await runTests();
