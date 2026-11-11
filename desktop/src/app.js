// SPDX-License-Identifier: MIT

/**
 * LiveAudio Tauri Desktop Frontend Application Logic
 */

import { adoptSavedStyle, audioStateKey, bindProfileSelection, bindTabNavigation, externalConfigUpdate, generalFields, hardwareStatusLabel, mergeSavedConfigIntoForm, monitorBannerText, monitorFeedAction, monitorFeedText, persistAndApplyRuntimeConfig, sessionFailureLabel, STYLE_CONFIG_KEYS, styleFields, engineKind, gpuReasonKey, nvidiaOfferVisible, nvidiaCardView, nvidiaInstallConfig, advancedView, TAB_RESETS } from './runtime-settings.js';
import { initLanguage, setLanguage, getLanguage, t, applyTranslations, getProfilePresetData, PROFILE_I18N } from './i18n.js';
import { controlsFromTheme, deletePreset, fontChoices, fontFamiliesForPlatform, loadPresetTheme, presetNameFromOption, presetOptionValue, previewHoldMs, previewProperties, previewWordDelays, savePreset, themeFromControls } from './subtitle-style.js';

// IPC Bridge
const isTauri = typeof window !== 'undefined' && Boolean(window.__TAURI__ && window.__TAURI__.core);

async function invoke(cmd, args = {}) {
  if (isTauri) {
    return await window.__TAURI__.core.invoke(cmd, args);
  }
  return mockInvoke(cmd, args);
}

async function listen(eventName, callback) {
  if (isTauri && window.__TAURI__.event) {
    return await window.__TAURI__.event.listen(eventName, callback);
  }
  // Mock listener stub
  return () => {};
}

// In-browser mock handler for isolated preview & testing
function mockInvoke(cmd, args) {
  console.log(`[Mock Tauri IPC] ${cmd}`, args);
  switch (cmd) {
    case 'get_config':
      return Promise.resolve({
        output_dir: 'E:\\LiveAudio\\sessions',
        device: 'cuda',
        cpu_threads: 8,
        model_size: 'small (Balance CPU)',
        blacklist: 'amara.org, subtítulos por, suscríbete, dale like, gracias por ver',
        continuous_session: true,
        subtitle_style: 'default',
        subtitle_theme: {},
        subtitle_presets: [],
        subtitle_display_mode: 'adaptive',
        subtitle_ribbon_max_lines: 3,
        subtitle_backlog_policy: 'auto',
        subtitle_max_live_delay_sec: 10.0,
        subtitle_catchup_interval_sec: 1.5,
        silence_timeout: 0.5,
        max_chunk_duration: 3.0,
        transcription_purpose: 'subtitles',
        asr_decode_timeout_sec: 15,
        audio_device: null,
        selected_profile_id: 'balanced',
        profile_mode: 'preset',
        ws_port: 8765,
        obs_enabled: true,
        mcp_style_enabled: false,
        mcp_system_enabled: false,
        check_updates_enabled: true,
        prewarm: true,
        save_transcript_enabled: true,
        save_vtt_enabled: true,
        whisper_context_prompt_es: '',
        whisper_context_prompt_en: '',
        asr_language: 'es',
        settings_navigation_mode: 'tabs',
        language: 'es',
        diagnostics_enabled: false,
        diagnostics_level: 'minimal',
        diagnostics_export_dir: null,
        last_update_check: 0,
        vad_speech_pad_ms: 200,
        vad_threshold: 0.5,
      });

    case 'save_subtitle_style':
      return Promise.resolve({
        subtitle_theme: args.theme || {},
        subtitle_presets: args.presets || [],
        subtitle_style: args.subtitleStyle || 'default',
      });

    case 'check_for_updates':
      return Promise.resolve(null);

    case 'install_update_now':
      return Promise.resolve();

    case 'set_install_on_close':
      return Promise.resolve();

    case 'take_startup_warning':
      return Promise.resolve(null);

    case 'get_updater_status':
      return Promise.resolve({
        current_version: '1.6.0',
        available_update: null,
        install_on_close: false,
        is_checking: false,
        is_installing: false,
        stream_busy: false,
        last_check: 0,
      });

    case 'save_config':
      return Promise.resolve(args.config || true);

    case 'prepare_asr_runtime':
      return Promise.resolve();

    case 'get_allowed_connections':
      return Promise.resolve({
        allowed_ports: [1420],
        recent_rejected: [],
      });

    case 'allow_loopback_port':
      return Promise.resolve({
        allowed_ports: [1420, args.port].filter((v, i, a) => a.indexOf(v) === i),
        recent_rejected: [],
      });

    case 'revoke_loopback_port':
      return Promise.resolve({
        allowed_ports: [1420].filter((p) => p !== args.port),
        recent_rejected: [],
      });

    case 'cancel_runtime_setup':
      return Promise.resolve(true);

    case 'get_profile_presets':
      return Promise.resolve([
        { id: 'fast', label: 'Rápido', description: 'Menos demora y frases cortas; baja un poco la precisión.', device: 'cpu', model_size: 'base (Rápido)', silence_timeout: 0.4, subtitle_backlog_policy: 'live_only', subtitle_max_live_delay_sec: 5, subtitle_catchup_interval_sec: 0.8 },
        { id: 'balanced', label: 'Balanceado', description: 'Recomendado para la mayoría de sesiones.', device: 'cuda', model_size: 'small (Balance CPU)', silence_timeout: 0.5, subtitle_backlog_policy: 'auto', subtitle_max_live_delay_sec: 10, subtitle_catchup_interval_sec: 1.5 },
        { id: 'quality', label: 'Calidad', description: 'Más precisión; puede usar más VRAM y tardar más.', device: 'cuda', model_size: 'turbo (Máxima precisión GPU)', silence_timeout: 1, subtitle_backlog_policy: 'auto', subtitle_max_live_delay_sec: 15, subtitle_catchup_interval_sec: 2 },
        { id: 'stable_streaming', label: 'Streaming estable', description: 'Reduce carga de GPU para jugar o transmitir en PC ocupada.', device: 'cpu', model_size: 'small (Balance CPU)', silence_timeout: 0.6, subtitle_backlog_policy: 'live_only', subtitle_max_live_delay_sec: 6, subtitle_catchup_interval_sec: 1 },
      ]);

    case 'open_session_folder':
      return Promise.resolve('E:\\LiveAudio\\sessions\\session-demo');

    case 'reset_config_defaults':
      return mockInvoke('get_config', {});

    case 'get_gpu_backend_status':
      return Promise.resolve({
        adapters: [{ vendor: 'nvidia', name: 'NVIDIA GeForce RTX 3060', dedicated_vram_mb: 12115 }],
        cuda_driver_version: 12080,
        nvidia_runtime: { installed: false, missing: ['cublas64_12.dll'] },
        license_urls: ['https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/LICENSE.txt'],
        selected_backend: 'ort',
        reason: 'nvidia_runtime_not_installed',
      });

    case 'install_nvidia_runtime':
      return Promise.reject('Preview mode: NVIDIA runtime install is only available in the desktop app.');

    case 'cancel_nvidia_runtime_install':
      return Promise.resolve(false);

    case 'get_audio_devices':
      return Promise.resolve([
        { id: 'default_microphone', name: 'Micrófono predeterminado del sistema', is_default: true, kind: 'input' },
        { id: 'wasapi_loopback', name: 'Audio del sistema / Altavoces (WASAPI Loopback)', is_default: false, kind: 'loopback' },
        { id: 'input_usb_mic', name: 'Micrófono USB HyperX SoloCast', is_default: false, kind: 'input' },
      ]);

    case 'get_service_status':
      return Promise.resolve({
        is_running: false,
        asr_state: 'stopped',
        vad_onset: false,
        ws_port: 8765,
        ws_clients: 0,
        active_device: null,
        model_size: 'small (Balance CPU)',
        device: 'cuda',
        uptime_sec: 0.0,
        restart_count: 0,
        session_path: null,
      });

    case 'start_service':
      return Promise.resolve({
        is_running: true,
        asr_state: 'listening',
        vad_onset: false,
        ws_port: 8765,
        ws_clients: 0,
        active_device: 'Micrófono predeterminado',
        model_size: 'small (Balance CPU)',
        device: 'cuda',
        uptime_sec: 0.5,
        restart_count: 0,
        session_path: 'E:\\LiveAudio\\sessions\\session-demo',
      });

    case 'stop_service':
      return Promise.resolve({
        is_running: false,
        asr_state: 'stopped',
        vad_onset: false,
        ws_port: 8765,
        ws_clients: 0,
        active_device: null,
        model_size: 'small (Balance CPU)',
        device: 'cuda',
        uptime_sec: 0.0,
        restart_count: 0,
        session_path: 'E:\\LiveAudio\\sessions\\session-demo',
      });

    case 'restart_service':
      return mockInvoke('start_service', {});

    case 'get_obs_overlay_info':
      return Promise.resolve({
        url: 'http://127.0.0.1:8765/overlay?port=8765',
        port: 8765,
        width: 1920,
        height: 1080,
        instructions: "En OBS Studio: Añadir fuente -> 'Navegador' (Browser Source). Ancho 1920, Alto 1080, FPS 60.",
      });

    case 'export_diagnostics':
      return Promise.resolve({
        path: 'E:\\LiveAudio\\sessions\\LiveAudio_Diagnostic_1700000000.json',
        timestamp: '1700000000',
        success: true,
      });

    case 'send_test_subtitle':
      return Promise.resolve({
        id: Date.now(),
        start: 0.0,
        end: 3.0,
        text: args.text || 'Hola mundo! Transcripción en tiempo real con LiveAudio.',
        style: args.style || 'default',
        timestamp_ms: Date.now(),
      });

    case 'get_recent_logs':
      return Promise.resolve([
        'LiveAudio Desktop Logging Initialized',
        'Log file: C:\\Users\\...\\logs\\liveaudio-desktop.log',
      ]);

    default:
      return Promise.reject(`Command not found: ${cmd}`);
  }
}

// Global App State
let currentConfig = null;
let savedConfig = null;
let profilePresets = [];
let currentStatus = {
  is_running: false,
  asr_state: 'stopped',
  vad_onset: false,
  ws_port: 8765,
  ws_clients: 0,
};
let subtitleClearTimeout = null;
let drafts = {
  prompts: {
    es: '',
    en: '',
  },
  blacklists: {
    es: 'amara.org, subtítulos por, suscríbete, dale like, gracias por ver, memos, gracias, activar la campanita',
    en: 'amara.org, subtitles by, subscribe, like, thank you for watching, thanks for watching, please subscribe',
  },
};
let currentAsrLang = 'es';

// Helper to handle tactile processing state on buttons
async function withProcessing(btn, asyncFn) {
  if (!btn) return await asyncFn();
  btn.classList.add('is-processing');
  try {
    return await asyncFn();
  } finally {
    btn.classList.remove('is-processing');
  }
}

// DOM Elements
const elements = {
  appContainer: document.getElementById('app-container'),
  authorProfileLink: document.getElementById('author-profile-link'),
  btnAbout: document.getElementById('btn-about'),
  aboutModal: document.getElementById('about-modal'),
  btnAboutClose: document.getElementById('btn-about-close'),
  btnAboutReport: document.getElementById('btn-about-report'),
  btnAboutStar: document.getElementById('btn-about-star'),
  btnAboutSponsor: document.getElementById('btn-about-sponsor'),
  aboutStatTime: document.getElementById('about-stat-time'),
  aboutStatSubtitles: document.getElementById('about-stat-subtitles'),
  aboutStatSessions: document.getElementById('about-stat-sessions'),
  aboutSince: document.getElementById('about-since'),
  aboutEmpty: document.getElementById('about-empty'),
  selectUiLang: document.getElementById('select-ui-lang'),
  pillStatus: document.getElementById('pill-status'),
  pillStatusText: document.getElementById('pill-status-text'),
  pillVad: document.getElementById('pill-vad'),
  pillVadText: document.getElementById('pill-vad-text'),
  pillWs: document.getElementById('pill-ws'),
  pillWsText: document.getElementById('pill-ws-text'),
  pillHw: document.getElementById('pill-hw'),
  btnToggleService: document.getElementById('btn-toggle-service'),
  btnToggleServiceText: document.getElementById('btn-toggle-service-text'),
  btnRuntimeRepair: document.getElementById('btn-runtime-repair'),
  btnRuntimeCancel: document.getElementById('btn-runtime-cancel'),
  runtimeSetupProgress: document.getElementById('runtime-setup-progress'),
  runtimeSetupText: document.getElementById('runtime-setup-text'),
  runtimeSetupMeter: document.getElementById('runtime-setup-meter'),
  btnSaveConfig: document.getElementById('btn-save-config'),
  btnResetConfig: document.getElementById('btn-reset-config'),
  btnExportDiag: document.getElementById('btn-export-diag'),
  audioDeviceSelect: document.getElementById('select-audio-device'),
  selectComputeDevice: document.getElementById('select-compute-device'),
  deviceRadioCpu: document.getElementById('device-cpu'),
  deviceRadioCuda: document.getElementById('device-cuda'),
  sliderCpuThreads: document.getElementById('slider-cpu-threads'),
  groupCpuThreads: document.getElementById('group-cpu-threads'),
  valCpuThreads: document.getElementById('val-cpu-threads'),
  sliderSilenceTimeout: document.getElementById('slider-silence-timeout'),
  valSilenceTimeout: document.getElementById('val-silence-timeout'),
  sliderMaxChunk: document.getElementById('slider-max-chunk'),
  valMaxChunk: document.getElementById('val-max-chunk'),
  selectModelSize: document.getElementById('select-model-size'),
  selectAsrLang: document.getElementById('select-asr-lang'),
  promptEs: document.getElementById('prompt-es'),
  btnSuggestPromptEs: document.getElementById('btn-suggest-prompt-es'),
  badgePromptLang: document.getElementById('badge-prompt-lang'),
  hintPrompt: document.getElementById('hint-prompt'),
  selectSubtitleStyle: document.getElementById('select-subtitle-style'),
  selectSubFont: document.getElementById('select-sub-font'),
  inputSubFontSize: document.getElementById('input-sub-font-size'),
  selectSubWeight: document.getElementById('select-sub-weight'),
  selectSubAnimation: document.getElementById('select-sub-animation'),
  inputSubAnimDuration: document.getElementById('input-sub-anim-duration'),
  inputSubLifetime: document.getElementById('input-sub-lifetime'),
  btnSubPreviewPlay: document.getElementById('btn-sub-preview-play'),
  inputSubLetterSpacing: document.getElementById('input-sub-letter-spacing'),
  valSubLetterSpacing: document.getElementById('val-sub-letter-spacing'),
  inputSubWordSpacing: document.getElementById('input-sub-word-spacing'),
  valSubWordSpacing: document.getElementById('val-sub-word-spacing'),
  inputSubColor: document.getElementById('input-sub-color'),
  inputSubBg: document.getElementById('input-sub-bg'),
  inputSubBgCustom: document.getElementById('check-sub-bg-custom'),
  inputSubBgOpacity: document.getElementById('input-sub-bg-opacity'),
  subStylePreview: document.getElementById('sub-style-preview'),
  subStylePreviewText: document.getElementById('sub-style-preview-text'),
  optgroupSubPresets: document.getElementById('optgroup-sub-presets'),
  inputSubPresetName: document.getElementById('input-sub-preset-name'),
  btnSubPresetSave: document.getElementById('btn-sub-preset-save'),
  btnSubPresetDelete: document.getElementById('btn-sub-preset-delete'),
  btnSubStyleReset: document.getElementById('btn-sub-style-reset'),
  chipSubStyleUnsaved: document.getElementById('chip-sub-style-unsaved'),
  chipAdvUnsaved: document.getElementById('chip-adv-unsaved'),
  subStyleStatus: document.getElementById('sub-style-status'),
  selectDisplayMode: document.getElementById('select-display-mode'),
  sliderRibbonLines: document.getElementById('slider-ribbon-lines'),
  valRibbonLines: document.getElementById('val-ribbon-lines'),
  selectBacklogPolicy: document.getElementById('select-backlog-policy'),
  selectAsrBackend: document.getElementById('select-asr-backend'),
  tabSetupTab: document.getElementById('tab-setup-tab'),
  enginePill: document.getElementById('engine-status-pill'),
  engineStatusText: document.getElementById('engine-status-text'),
  engineStatusReason: document.getElementById('engine-status-reason'),
  btnNvidiaAdvanced: document.getElementById('btn-nvidia-advanced'),
  gpuStatusText: document.getElementById('gpu-status-text'),
  cardNvidiaRuntime: document.getElementById('card-nvidia-runtime'),
  nvidiaOffer: document.getElementById('nvidia-offer'),
  nvidiaGpuName: document.getElementById('nvidia-gpu-name'),
  btnNvidiaEnable: document.getElementById('btn-nvidia-enable'),
  btnNvidiaDismiss: document.getElementById('btn-nvidia-dismiss'),
  nvidiaInstallProgress: document.getElementById('nvidia-install-progress'),
  nvidiaInstallDetail: document.getElementById('nvidia-install-detail'),
  nvidiaInstallMeter: document.getElementById('nvidia-install-meter'),
  btnNvidiaCancel: document.getElementById('btn-nvidia-cancel'),
  nvidiaResult: document.getElementById('nvidia-result'),
  nvidiaResultTitle: document.getElementById('nvidia-result-title'),
  nvidiaResultText: document.getElementById('nvidia-result-text'),
  nvidiaErrorDetails: document.getElementById('nvidia-error-details'),
  nvidiaErrorText: document.getElementById('nvidia-error-text'),
  btnNvidiaRestart: document.getElementById('btn-nvidia-restart'),
  btnNvidiaRetry: document.getElementById('btn-nvidia-retry'),
  sliderMaxLiveDelay: document.getElementById('slider-max-live-delay'),
  sliderCatchupInterval: document.getElementById('slider-catchup-interval'),
  valCatchupInterval: document.getElementById('val-catchup-interval'),
  selectProfile: document.getElementById('select-profile'),
  profileDescription: document.getElementById('profile-description'),
  valMaxLiveDelay: document.getElementById('val-max-live-delay'),
  inputWsPort: document.getElementById('input-ws-port'),
  sliderVadPad: document.getElementById('slider-vad-pad'),
  valVadPad: document.getElementById('val-vad-pad'),
  sliderVadThreshold: document.getElementById('slider-vad-threshold'),
  valVadThreshold: document.getElementById('val-vad-threshold'),
  inputBlacklist: document.getElementById('input-blacklist'),
  badgeBlacklistLang: document.getElementById('badge-blacklist-lang'),
  hintBlacklist: document.getElementById('hint-blacklist'),
  checkContinuous: document.getElementById('check-continuous'),
  checkSaveTranscript: document.getElementById('check-save-transcript'),
  checkSaveVtt: document.getElementById('check-save-vtt'),
  checkRnnoiseEnabled: document.getElementById('check-rnnoise-enabled'),
  checkMcpStyleEnabled: document.getElementById('check-mcp-style-enabled'),
  checkMcpSystemEnabled: document.getElementById('check-mcp-system-enabled'),
  checkAcousticEventsEnabled: document.getElementById('check-acoustic-events-enabled'),
  checkLivePartialSubtitles: document.getElementById('check-live-partial-subtitles-enabled'),
  feedPartialLine: document.getElementById('feed-partial-line'),
  sessionPath: document.getElementById('session-path'),
  sessionError: document.getElementById('session-error'),
  monitorBanner: document.getElementById('monitor-banner'),
  btnOpenSessionFolder: document.getElementById('btn-open-session-folder'),
  cardSubtitlesFeed: document.getElementById('card-subtitles-feed'),
  subtitlesFeedContainer: document.getElementById('subtitles-feed-container'),
  feedEmptyState: document.getElementById('feed-empty-state'),
  feedCountBadge: document.getElementById('feed-count-badge'),
  feedLiveBadge: document.getElementById('feed-live-badge'),
  btnTestSubtitle: document.getElementById('btn-test-subtitle'),
  btnClearFeed: document.getElementById('btn-clear-feed'),
  feedActionMenu: document.getElementById('feed-action-menu'),
  inputObsUrl: document.getElementById('input-obs-url'),
  btnCopyObsUrl: document.getElementById('btn-copy-obs-url'),
  toastContainer: document.getElementById('toast-container'),
  winBtnMinimize: document.getElementById('win-btn-minimize'),
  winBtnMaximize: document.getElementById('win-btn-maximize'),
  winBtnClose: document.getElementById('win-btn-close'),
  winIconRect: document.getElementById('win-icon-rect'),
  loggerCard: document.getElementById('card-logger'),
  loggerConsole: document.getElementById('logger-console'),
  loggerCountBadge: document.getElementById('logger-count-badge'),
  btnToggleLogger: document.getElementById('btn-toggle-logger'),
  btnClearLogs: document.getElementById('btn-clear-logs'),
  btnCopyLogs: document.getElementById('btn-copy-logs'),
  groupUpdates: document.getElementById('group-updates'),
  checkUpdatesEnabled: document.getElementById('check-updates-enabled'),
  btnCheckNow: document.getElementById('btn-check-now'),
  updaterStatus: document.getElementById('updater-status'),
  badgeHeaderUpdate: document.getElementById('badge-header-update'),
  badgeHeaderUpdateText: document.getElementById('badge-header-update-text'),
  cardReleaseNotes: document.getElementById('card-release-notes'),
  updateReleaseNotes: document.getElementById('release-notes-body'),
  updateAvailableTitle: document.getElementById('update-available-title'),
  btnInstallNow: document.getElementById('btn-install-now'),
  btnInstallOnClose: document.getElementById('btn-install-on-close'),
  statusUpdateBusy: document.getElementById('status-update-busy'),
  allowedPortsList: document.getElementById('allowed-ports-list'),
  inputAddAllowedPort: document.getElementById('input-add-allowed-port'),
  btnAddAllowedPort: document.getElementById('btn-add-allowed-port'),
  errorAddAllowedPort: document.getElementById('error-add-allowed-port'),
  recentBlockedList: document.getElementById('recent-blocked-list'),
};

// Logger Console Subsystem
const MAX_LOG_ENTRIES = 300;

function appendLogEntry(level, message, source = '') {
  if (!elements.loggerConsole) return;

  const now = new Date();
  const timeStr = [
    String(now.getHours()).padStart(2, '0'),
    String(now.getMinutes()).padStart(2, '0'),
    String(now.getSeconds()).padStart(2, '0'),
  ].join(':');

  const line = document.createElement('div');
  line.className = `log-line log-${level.toLowerCase()}`;

  const timeSpan = document.createElement('span');
  timeSpan.className = 'log-timestamp';
  timeSpan.textContent = timeStr;

  const tagSpan = document.createElement('span');
  tagSpan.className = `log-tag tag-${level.toLowerCase()}`;
  tagSpan.textContent = source || level.toUpperCase();

  const msgSpan = document.createElement('span');
  msgSpan.className = 'log-msg';
  msgSpan.textContent = message;

  line.appendChild(timeSpan);
  line.appendChild(tagSpan);
  line.appendChild(msgSpan);

  elements.loggerConsole.appendChild(line);

  while (elements.loggerConsole.children.length > MAX_LOG_ENTRIES) {
    elements.loggerConsole.removeChild(elements.loggerConsole.firstChild);
  }

  if (elements.loggerCountBadge) {
    elements.loggerCountBadge.textContent = String(elements.loggerConsole.children.length);
  }

  elements.loggerConsole.scrollTop = elements.loggerConsole.scrollHeight;
}

function clearLogs() {
  if (!elements.loggerConsole) return;
  elements.loggerConsole.innerHTML = '';
  if (elements.loggerCountBadge) elements.loggerCountBadge.textContent = '0';
}

async function copyLogs() {
  if (!elements.loggerConsole) return;
  const lines = Array.from(elements.loggerConsole.querySelectorAll('.log-line')).map((el) => {
    const time = el.querySelector('.log-timestamp')?.textContent || '';
    const tag = el.querySelector('.log-tag')?.textContent || '';
    const msg = el.querySelector('.log-msg')?.textContent || '';
    return `[${time}] [${tag}] ${msg}`;
  });
  try {
    await navigator.clipboard.writeText(lines.join('\n'));
    showToast(t('toast_logs_copied') || 'Registro copiado');
  } catch (err) {
    showToast(`Error: ${err}`, true);
  }
}

let isLoggerCollapsed = true;

function toggleLoggerCollapse() {
  isLoggerCollapsed = !isLoggerCollapsed;
  if (elements.loggerCard) {
    elements.loggerCard.classList.toggle('collapsed', isLoggerCollapsed);
  }
  if (elements.loggerConsole) {
    elements.loggerConsole.hidden = isLoggerCollapsed;
  }
  if (elements.btnToggleLogger) {
    const key = isLoggerCollapsed ? 'btn_toggle_logger_show' : 'btn_toggle_logger_hide';
    elements.btnToggleLogger.textContent = t(key);
    elements.btnToggleLogger.setAttribute('data-i18n', key);
    elements.btnToggleLogger.setAttribute('aria-expanded', String(!isLoggerCollapsed));
  }
}

function updateComputeDeviceReactivity(device) {
  const currentDevice = device || (elements.selectComputeDevice ? elements.selectComputeDevice.value : 'cpu');
  const isCuda = currentDevice === 'cuda';
  const group = elements.groupCpuThreads || elements.sliderCpuThreads?.closest('.form-group');

  if (elements.sliderCpuThreads) {
    elements.sliderCpuThreads.disabled = isCuda;
  }

  if (group) {
    group.classList.toggle('device-dimmed', isCuda);
    const hint = group.querySelector('.form-hint');
    if (hint) {
      const hintKey = isCuda ? 'hint_cpu_threads_cuda' : 'hint_cpu_threads';
      hint.setAttribute('data-i18n', hintKey);
      hint.textContent = t(hintKey);
    }
  }
}

// Asks before closing while this window owns a running session. Monitor Mode and idle windows
// close without a prompt. Confirm re-enters close, so the ExitRequested stop runs exactly once.
let closeConfirmed = false;
let closePromptOpen = false;
let saveGeneralConfig = async () => {};
const ADVANCED_FIELD_IDS = ['select-model-size', 'select-compute-device', 'slider-cpu-threads', 'select-asr-backend', 'slider-silence-timeout', 'slider-max-chunk',
  'slider-vad-pad', 'slider-vad-threshold', 'check-rnnoise-enabled', 'check-live-partial-subtitles-enabled', 'select-backlog-policy', 'slider-max-live-delay',
  'slider-catchup-interval', 'prompt-es', 'input-blacklist', 'input-ws-port', 'check-acoustic-events-enabled', 'check-mcp-style-enabled', 'check-mcp-system-enabled', 'check-updates-enabled'];
function setupCloseGuard(appWindow) {
  if (!appWindow || typeof appWindow.onCloseRequested !== 'function') return;
  appWindow.onCloseRequested(async (event) => {
    if (closeConfirmed) return;
    const asksToStop = currentStatus.is_running && !currentStatus.monitor_mode;
    const asksToSave = styleChangesUnsaved();
    const asksToSaveAdvanced = advancedChangesUnsaved();
    if (!asksToStop && !asksToSave && !asksToSaveAdvanced && nvidiaCardState !== 'installing' && !styleDirty) return;
    event.preventDefault();
    if (closePromptOpen) return;
    closePromptOpen = true;
    try {
      // Save or discard the unsaved saved-style edits first; the stop prompt below can still cancel the close.
      if (asksToSave) {
        await confirmLeaveStyleTab();
      }
      if (subtitleStyleSaveTimer !== null) { clearTimeout(subtitleStyleSaveTimer); await queueConfigSave(saveSubtitleStyle); } // write before the window goes
      if (asksToSaveAdvanced && !(await confirmLeaveAdvancedTab())) return; // the save did not go through: stay
      // Exit also cancels an NVIDIA install; a style save that failed would be lost with the window.
      const warn = asksToStop ? 'confirm' : nvidiaCardState === 'installing' ? 'install' : styleDirty ? 'style' : null;
      const ok = !warn || await showConfirmModal({
        title: t('close_confirm_title'),
        message: t(`close_${warn}_message`),
        detail: t(`close_${warn}_detail`),
        confirmText: t(`close_${warn}_action`),
        isDanger: true,
      });
      if (ok) {
        closeConfirmed = true;
        await appWindow.close();
      }
    } catch (err) {
      closeConfirmed = false;
      console.warn('Close error:', err);
    } finally {
      closePromptOpen = false;
    }
  });
}

// Custom Frameless Window Controls Integration
function setupWindowControls() {
  const getAppWindow = () => {
    try {
      if (window.__TAURI__ && window.__TAURI__.window) {
        return window.__TAURI__.window.getCurrentWindow();
      }
    } catch (_) {}
    return null;
  };

  const appWindow = getAppWindow();
  setupCloseGuard(appWindow);

  if (elements.winBtnMinimize) {
    elements.winBtnMinimize.addEventListener('click', async () => {
      try {
        if (appWindow) await appWindow.minimize();
      } catch (e) {
        console.warn('Minimize error:', e);
      }
    });
  }

  if (elements.winBtnMaximize) {
    elements.winBtnMaximize.addEventListener('click', async () => {
      try {
        if (appWindow) await appWindow.toggleMaximize();
      } catch (e) {
        console.warn('Toggle maximize error:', e);
      }
    });
  }

  if (elements.winBtnClose) {
    elements.winBtnClose.addEventListener('click', async () => {
      try {
        if (appWindow) await appWindow.close();
      } catch (e) {
        console.warn('Close error:', e);
      }
    });
  }

  if (appWindow && appWindow.onResized) {
    try {
      appWindow.onResized(async () => {
        try {
          const isMax = await appWindow.isMaximized();
          if (elements.winBtnMaximize) {
            elements.winBtnMaximize.title = isMax ? t('btn_maximize') : t('btn_maximize');
          }
        } catch (_) {}
      });
    } catch (_) {}
  }
}

// UI Notification
function showToast(message, type = false) {
  const toast = document.createElement('div');
  const typeClass = type === true || type === 'error'
    ? 'toast-error'
    : type === 'warning'
    ? 'toast-warning'
    : '';
  toast.className = `toast ${typeClass}`.trim();
  toast.textContent = message;
  elements.toastContainer.appendChild(toast);
  setTimeout(() => {
    toast.remove();
  }, 3500);
}

// In-App Updater Subsystem
let availableUpdate = null;
let installOnCloseArmed = false;
let updateStatusClearTimer = null;

function isStreamBusy(status = currentStatus) {
  if (!status) return false;
  return Boolean(
    status.is_running
    || (status.ws_clients && status.ws_clients > 0)
    || status.monitor_mode
    || status.foreign_service_pid
    || (status.asr_state && status.asr_state !== 'stopped')
  );
}

function syncUpdaterUI() {
  const busy = isStreamBusy(currentStatus);
  const hasUpdate = Boolean(availableUpdate && availableUpdate.version);

  if (elements.badgeHeaderUpdate) {
    if (hasUpdate && !busy) {
      elements.badgeHeaderUpdate.hidden = false;
      if (elements.badgeHeaderUpdateText) {
        elements.badgeHeaderUpdateText.textContent = t('update_pill').replace('{version}', availableUpdate.version);
      }
    } else {
      elements.badgeHeaderUpdate.hidden = true;
    }
  }

  if (elements.cardReleaseNotes) {
    elements.cardReleaseNotes.hidden = !hasUpdate;
  }

  if (elements.btnInstallNow) {
    elements.btnInstallNow.disabled = busy || !hasUpdate;
  }
  if (elements.statusUpdateBusy) {
    elements.statusUpdateBusy.hidden = !hasUpdate || !busy;
  }

  if (elements.btnInstallOnClose) {
    elements.btnInstallOnClose.setAttribute('aria-pressed', String(installOnCloseArmed));
    elements.btnInstallOnClose.classList.toggle('active', installOnCloseArmed);
  }
}

function renderReleaseNotes(update) {
  if (!elements.cardReleaseNotes) return;
  if (!update || !update.version) {
    elements.cardReleaseNotes.hidden = true;
    return;
  }

  if (elements.updateAvailableTitle) {
    elements.updateAvailableTitle.textContent = t('status_update_available').replace('{version}', update.version);
  }

  if (elements.updateReleaseNotes) {
    const notes = (update.notes || '').trim();
    elements.updateReleaseNotes.textContent = notes || (getLanguage() === 'es' ? 'Sin notas de la versión.' : 'No release notes provided.');
  }

  syncUpdaterUI();
}

async function checkForUpdatesManual() {
  if (elements.btnCheckNow) elements.btnCheckNow.disabled = true;
  if (elements.updaterStatus) {
    elements.updaterStatus.textContent = t('status_checking_updates');
    elements.updaterStatus.className = 'updater-status-text status-checking';
  }
  clearTimeout(updateStatusClearTimer);

  try {
    const update = await invoke('check_for_updates', { force: true });
    if (update && update.version) {
      availableUpdate = update;
      if (elements.updaterStatus) {
        elements.updaterStatus.textContent = t('status_update_available').replace('{version}', update.version);
        elements.updaterStatus.className = 'updater-status-text status-available';
      }
      renderReleaseNotes(update);
    } else {
      availableUpdate = null;
      if (elements.updaterStatus) {
        elements.updaterStatus.textContent = t('status_up_to_date');
        elements.updaterStatus.className = 'updater-status-text status-uptodate';
      }
      if (elements.cardReleaseNotes) elements.cardReleaseNotes.hidden = true;
    }
  } catch (err) {
    console.error('Update check failed:', err);
    if (elements.updaterStatus) {
      elements.updaterStatus.textContent = t('err_update_offline');
      elements.updaterStatus.className = 'updater-status-text status-error';
    }
  } finally {
    if (elements.btnCheckNow) elements.btnCheckNow.disabled = false;
    syncUpdaterUI();
    updateStatusClearTimer = setTimeout(() => {
      if (elements.updaterStatus && !availableUpdate) {
        elements.updaterStatus.textContent = '';
        elements.updaterStatus.className = 'updater-status-text';
      }
    }, 8000);
  }
}

async function installUpdateNow() {
  if (isStreamBusy(currentStatus)) {
    showToast(t('status_update_deferred_busy'), true);
    return;
  }
  if (elements.btnInstallNow) elements.btnInstallNow.disabled = true;
  if (elements.btnCheckNow) elements.btnCheckNow.disabled = true;
  try {
    await invoke('install_update_now');
  } catch (err) {
    console.error('Failed to install update:', err);
    showToast(`${t('status_error')}: ${err}`, true);
  } finally {
    syncUpdaterUI();
  }
}

async function toggleInstallOnClose() {
  const nextState = !installOnCloseArmed;
  try {
    await invoke('set_install_on_close', { enabled: nextState });
    installOnCloseArmed = nextState;
    if (elements.btnInstallOnClose) {
      elements.btnInstallOnClose.setAttribute('aria-pressed', String(installOnCloseArmed));
      elements.btnInstallOnClose.classList.toggle('active', installOnCloseArmed);
    }
  } catch (err) {
    console.error('Failed to set install on close:', err);
    showToast(`${t('status_error')}: ${err}`, true);
  }
}

// Hardware fallback tracking
let hasNotifiedHardwareFallback = false;

// Subtitle feed auto-activation and socket tracking
let feedPreviewAutoStarted = false;
let lastRenderedCue = { text: '', utterance_id: null, timestamp: 0 };

function startFeedPreview(port) {
  if (monitorFeed && monitorFeed.port === port) return;
  if (monitorFeed) {
    try { monitorFeed.socket.close(); } catch (_) {}
    monitorFeed = null;
    feedPreviewAutoStarted = false;
  }
  try {
    const feed = { socket: new WebSocket(`ws://127.0.0.1:${port}`), port };
    feed.socket.onmessage = (event) => addSubtitleToFeed(monitorFeedText(event.data));
    feed.socket.onclose = () => {
      if (monitorFeed === feed) {
        monitorFeed = null;
        feedPreviewAutoStarted = false;
      }
    };
    monitorFeed = feed;
    feedPreviewAutoStarted = true;
  } catch (err) {
    console.warn('Could not start feed preview WebSocket:', err);
  }
}

function stopFeedPreview() {
  if (monitorFeed) {
    try { monitorFeed.socket.close(); } catch (_) {}
    monitorFeed = null;
    feedPreviewAutoStarted = false;
  }
}

// Master button transition state tracking to prevent race conditions and enforce idempotency
let isServiceActionPending = false;
let serviceActionPendingMode = null; // 'starting' | 'stopping'

// Update Status Pills and Control Bar
function updateStatusUI(status) {
  currentStatus = status;
  renderGpuStatus();

  if (status.asr_progress) {
    setRuntimeSetupProgress({
      phase: 'downloading',
      text: status.asr_progress.text,
      percent: status.asr_progress.percent,
    });
  } else if (['ready', 'listening', 'stopped'].includes(status.asr_state)) {
    setRuntimeSetupProgress({ phase: 'ready', text: '', percent: null });
  } else if (status.asr_state && status.asr_state.startsWith('error')) {
    setRuntimeSetupProgress({ phase: 'error', text: status.asr_state, percent: null });
  }

  // Master Toggle Button - Real-time state handling & anti-race conditions
  const isTransitioning = isServiceActionPending
    || status.asr_state === 'starting'
    || status.asr_state === 'stopping'
    || status.asr_state === 'downloading'
    || Boolean(status.asr_progress)
    || (status.asr_state && status.asr_state.startsWith('Descargando'));

  if (isTransitioning) {
    const mode = (serviceActionPendingMode === 'stopping' || status.asr_state === 'stopping')
      ? 'stopping'
      : 'starting';
    setServiceButtonLoading(mode);
  } else if (status.monitor_mode) {
    // Another process owns the service: status only, never start/stop it from here.
    elements.btnToggleService.disabled = true;
    elements.btnToggleService.className = 'btn-toggle-service loading';
    elements.btnToggleService.innerHTML = `<span id="btn-toggle-service-text">${t('btn_monitor')}</span>`;
  } else if (status.is_running) {
    elements.btnToggleService.disabled = false;
    elements.btnToggleService.className = 'btn-toggle-service stop';
    elements.btnToggleService.innerHTML = `
      <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
        <rect x="6" y="6" width="12" height="12" rx="2"></rect>
      </svg>
      <span id="btn-toggle-service-text">${t('btn_stop')}</span>
    `;
  } else {
    elements.btnToggleService.disabled = false;
    elements.btnToggleService.className = 'btn-toggle-service start';
    elements.btnToggleService.innerHTML = `
      <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
        <polygon points="5 3 19 12 5 21 5 3"></polygon>
      </svg>
      <span id="btn-toggle-service-text">${t('btn_start')}</span>
    `;
  }

  // ASR State Pill
  const state = status.monitor_mode ? 'monitor' : (status.asr_state || (status.is_running ? 'listening' : 'stopped'));
  const isAudioStandbyOrReconnecting = Boolean(
    status.is_running && (status.audio_state === 'standby' || status.audio_state === 'reconnecting')
  );
  const cssState = state === 'monitor'
    ? 'listening'
    : state.startsWith('error')
    ? 'failed'
    : isAudioStandbyOrReconnecting
    ? 'degraded'
    : (state.startsWith('Descargando') || state === 'downloading' || state === 'loading')
    ? 'starting'
    : state;
  elements.pillStatus.className = `pill state-${cssState}`;
  const stateLabels = {
    stopped: t('status_stopped'),
    starting: t('status_starting'),
    loading: t('status_loading'),
    downloading: t('status_downloading'),
    ready: t('status_ready'),
    listening: t('status_listening'),
    transcribing: t('status_transcribing'),
    degraded: t('status_degraded'),
    failed: t('status_failed'),
    monitor: t('status_monitor'),
  };
  let pillText = stateLabels[state] || state;
  if (isAudioStandbyOrReconnecting) {
    const audioLabel = t(`audio_${status.audio_state}`) || status.audio_state;
    pillText += ` | Audio: ${audioLabel}`;
  }
  elements.pillStatusText.textContent = pillText;

  // VAD Pill
  if (status.vad_onset) {
    elements.pillVad.classList.add('active');
    elements.pillVadText.textContent = t('vad_speech');
  } else {
    elements.pillVad.classList.remove('active');
    elements.pillVadText.textContent = t('vad_silence');
  }

  // WS Pill
  elements.pillWsText.textContent = status.ws_port === 0
    ? `WS: ${t('ws_off')}`
    : `WS: ${status.ws_port} (${status.ws_clients} ${t('ws_clients_suffix')})`;

  // Hardware Pill
  const audioKey = audioStateKey(status);
  const hwLabel = audioKey ? `${hardwareStatusLabel(status)} | ${t(audioKey)}` : hardwareStatusLabel(status);
  elements.pillHw.textContent = hwLabel;

  const isFallback = Boolean(status.is_running && status.fallback_occurred);
  elements.pillHw.classList.toggle('hw-fallback', isFallback);

  if (isFallback) {
    elements.pillHw.title = status.fallback_reason || t('hw_fallback_warning') || 'GPU fallback occurred';
  } else if (status.is_running && status.effective_device) {
    const eff = String(status.effective_device).toLowerCase();
    if (eff === 'cuda') {
      elements.pillHw.title = t('hw_active_cuda') || 'GPU activa (CUDA)';
    } else if (eff === 'directml' || eff === 'dml') {
      elements.pillHw.title = t('hw_active_directml') || 'GPU activa (DirectML)';
    } else {
      elements.pillHw.title = hwLabel;
    }
  } else {
    elements.pillHw.title = hwLabel;
  }

  if (!status.is_running) {
    hasNotifiedHardwareFallback = false;
  } else if (status.fallback_occurred && !hasNotifiedHardwareFallback) {
    hasNotifiedHardwareFallback = true;
    const reason = status.fallback_reason || t('hw_fallback_warning') || 'GPU no disponible; usando CPU como respaldo';
    appendLogEntry('warn', `Fallback de hardware: ${reason}`, 'ASR');
    showToast(reason, 'warning');
  }

  if (elements.sessionPath) {
    elements.sessionPath.textContent = status.session_path || (currentConfig && currentConfig.output_dir) || t('lbl_no_session');
    elements.btnOpenSessionFolder.disabled = false;
  }
  if (elements.sessionError) {
    elements.sessionError.textContent = sessionFailureLabel(status);
    elements.sessionError.hidden = !sessionFailureLabel(status);
  }
  if (elements.monitorBanner) {
    elements.monitorBanner.textContent = monitorBannerText(status, t);
    elements.monitorBanner.hidden = !status.monitor_mode;
  }

  // Live Subtitle Feed Auto-Activation
  const isFeedLive = Boolean(status.is_running || status.monitor_mode || (monitorFeed && monitorFeed.socket));
  if (elements.cardSubtitlesFeed) {
    elements.cardSubtitlesFeed.classList.toggle('feed-active', isFeedLive);
  }
  if (elements.feedLiveBadge) {
    elements.feedLiveBadge.className = isFeedLive ? 'badge badge-feed-live' : 'badge badge-feed-idle';
    elements.feedLiveBadge.textContent = isFeedLive ? t('feed_badge_live') : t('feed_badge_idle');
    elements.feedLiveBadge.setAttribute('data-i18n', isFeedLive ? 'feed_badge_live' : 'feed_badge_idle');
  }
  if (elements.feedEmptyState && elements.subtitlesFeedContainer && !elements.subtitlesFeedContainer.querySelector('.feed-item')) {
    elements.feedEmptyState.textContent = isFeedLive
      ? (t('feed_listening_live') || 'Escuchando audio en vivo — habla para ver los subtítulos en tiempo real...')
      : (t('feed_empty') || 'Los subtítulos transcritos aparecerán aquí...');
    elements.feedEmptyState.hidden = false;
  }

  syncUpdaterUI();
  syncMonitorFeed(status);
}

// Read-only subtitle feed from a service another process owns (same protocol as the OBS overlay).
let monitorFeed = null; // { socket, port }

function syncMonitorFeed(status) {
  const action = monitorFeedAction(status, monitorFeed ? monitorFeed.port : null);
  if (action === 'keep') return;
  if (monitorFeed) {
    stopFeedPreview();
  }
  if (action !== 'open') return;
  if (status.ws_port) {
    startFeedPreview(status.ws_port);
  }
}

// Stream-Chat Subtitle Feed Subsystem
const MAX_FEED_ITEMS = 120;
let activeFeedMenuContext = null;
let configSaveQueue = Promise.resolve();

function queueConfigSave(saveTask) {
  const next = configSaveQueue.then(saveTask, saveTask);
  configSaveQueue = next.catch(() => {});
  return next;
}

function formatTimestamp(timestampMs) {
  const d = timestampMs ? new Date(timestampMs) : new Date();
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

// Provisional text for the phrase being spoken; lives outside the feed so dedupe never sees it.
function showFeedPartial(text) {
  const line = elements.feedPartialLine;
  if (!line) return;
  const clean = (text || '').trim();
  line.textContent = clean;
  line.hidden = !clean;
}

function addSubtitleToFeed(text, cue = null) {
  if (!elements.subtitlesFeedContainer) return;
  const cleanText = (text || '').trim();
  if (!cleanText) return;

  const now = Date.now();
  const utteranceId = cue?.utterance_id || null;
  if (utteranceId && utteranceId === lastRenderedCue.utterance_id) {
    return;
  }
  if (cleanText === lastRenderedCue.text && (now - lastRenderedCue.timestamp) < 350) {
    return;
  }
  lastRenderedCue = { text: cleanText, utterance_id: utteranceId, timestamp: now };

  if (elements.feedEmptyState) {
    elements.feedEmptyState.hidden = true;
  }

  const container = elements.subtitlesFeedContainer;
  const isScrolledToBottom = (container.scrollHeight - container.scrollTop - container.clientHeight) <= 40;

  const item = document.createElement('div');
  item.className = 'feed-item';
  const timestampMs = cue?.timestamp_ms || Date.now();
  item.dataset.timestamp = String(timestampMs);
  item.dataset.text = cleanText;

  const timeSpan = document.createElement('span');
  timeSpan.className = 'feed-item-time';
  timeSpan.textContent = formatTimestamp(timestampMs);

  const body = document.createElement('div');
  body.className = 'feed-item-body';

  const textSpan = document.createElement('span');
  textSpan.className = 'feed-item-text';
  textSpan.textContent = cleanText;
  body.appendChild(textSpan);

  const menuBtn = document.createElement('button');
  menuBtn.className = 'btn-feed-item-menu';
  menuBtn.type = 'button';
  menuBtn.setAttribute('aria-haspopup', 'true');
  menuBtn.setAttribute('aria-expanded', 'false');
  const menuLabel = t('feed_item_menu_title') || 'Opciones de subtítulo';
  menuBtn.setAttribute('aria-label', menuLabel);
  menuBtn.title = menuLabel;
  menuBtn.setAttribute('data-i18n-aria-label', 'feed_item_menu_title');
  menuBtn.setAttribute('data-i18n-title', 'feed_item_menu_title');
  menuBtn.innerHTML = `<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><circle cx="12" cy="12" r="1.5"></circle><circle cx="12" cy="5" r="1.5"></circle><circle cx="12" cy="19" r="1.5"></circle></svg>`;

  menuBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    openFeedActionMenu(cleanText, item, menuBtn);
  });

  item.appendChild(timeSpan);
  item.appendChild(body);
  item.appendChild(menuBtn);

  container.appendChild(item);

  // Circular buffer guard: maintain at most MAX_FEED_ITEMS elements
  const allItems = container.querySelectorAll('.feed-item');
  if (allItems.length > MAX_FEED_ITEMS) {
    const toRemove = allItems.length - MAX_FEED_ITEMS;
    let prunedHeight = 0;
    for (let i = 0; i < toRemove; i++) {
      if (allItems[i]) {
        if (activeFeedMenuContext && activeFeedMenuContext.item === allItems[i]) {
          closeFeedActionMenu();
        }
        if (!isScrolledToBottom) {
          prunedHeight += allItems[i].offsetHeight || 0;
        }
        allItems[i].remove();
      }
    }
    if (!isScrolledToBottom && prunedHeight > 0) {
      container.scrollTop = Math.max(0, container.scrollTop - prunedHeight);
    }
  }

  if (elements.feedCountBadge) {
    elements.feedCountBadge.textContent = String(container.querySelectorAll('.feed-item').length);
  }

  if (isScrolledToBottom) {
    container.scrollTop = container.scrollHeight;
  }
}

function openFeedActionMenu(text, itemElement, buttonElement) {
  if (!elements.feedActionMenu) return;

  if (activeFeedMenuContext && activeFeedMenuContext.button === buttonElement && !elements.feedActionMenu.hidden) {
    closeFeedActionMenu();
    return;
  }

  activeFeedMenuContext = {
    text,
    item: itemElement,
    button: buttonElement,
  };

  buttonElement.setAttribute('aria-expanded', 'true');
  elements.feedActionMenu.hidden = false;

  const rect = buttonElement.getBoundingClientRect();
  const menuWidth = elements.feedActionMenu.offsetWidth || 220;
  const menuHeight = elements.feedActionMenu.offsetHeight || 135;
  const margin = 8;
  const viewportWidth = window.innerWidth || document.documentElement.clientWidth || 800;
  const viewportHeight = window.innerHeight || document.documentElement.clientHeight || 600;

  let left = rect.right - menuWidth;
  if (left + menuWidth > viewportWidth - margin) {
    left = viewportWidth - menuWidth - margin;
  }
  if (left < margin) {
    left = Math.max(margin, rect.left);
  }

  let top = rect.bottom + 4;
  if (top + menuHeight > viewportHeight - margin) {
    top = Math.max(margin, rect.top - menuHeight - 4);
  }
  if (top < margin) {
    top = margin;
  }

  elements.feedActionMenu.style.top = `${top}px`;
  elements.feedActionMenu.style.left = `${left}px`;

  const firstBtn = elements.feedActionMenu.querySelector('.feed-menu-btn');
  if (firstBtn) firstBtn.focus();
}

function closeFeedActionMenu() {
  if (!elements.feedActionMenu) return;
  elements.feedActionMenu.hidden = true;
  if (activeFeedMenuContext?.button) {
    activeFeedMenuContext.button.setAttribute('aria-expanded', 'false');
    activeFeedMenuContext.button.focus();
  }
  activeFeedMenuContext = null;
}

async function handleFeedAction(action) {
  if (!activeFeedMenuContext) return;
  const { text, item } = activeFeedMenuContext;

  if (action === 'blacklist') {
    closeFeedActionMenu();
    await addPhraseToBlacklist(text, item);
  } else if (action === 'prompt') {
    closeFeedActionMenu();
    await addTermToInitialPrompt(text);
  } else if (action === 'copy') {
    closeFeedActionMenu();
    await copyTextToClipboard(text);
  }
}

function markFeedItemsBlocked(phrase) {
  if (!elements.subtitlesFeedContainer) return;
  const lowerPhrase = (phrase || '').trim().toLowerCase();
  if (!lowerPhrase) return;
  const items = elements.subtitlesFeedContainer.querySelectorAll('.feed-item');
  items.forEach((item) => {
    const text = (item.dataset.text || '').toLowerCase();
    if (text.includes(lowerPhrase)) {
      item.classList.add('feed-item-blocked');
      const body = item.querySelector('.feed-item-body');
      if (body && !body.querySelector('.feed-item-badge-blocked')) {
        const badge = document.createElement('span');
        badge.className = 'feed-item-badge-blocked';
        badge.setAttribute('data-i18n', 'badge_blocked');
        badge.textContent = t('badge_blocked') || 'Baneado';
        body.appendChild(badge);
      }
    }
  });
}

async function addPhraseToBlacklist(text, itemElement) {
  const trimmed = text.trim();
  if (!trimmed) return;

  const langKey = currentAsrLang === 'en' ? 'en' : 'es';
  if (elements.inputBlacklist) {
    drafts.blacklists[langKey] = elements.inputBlacklist.value;
  }
  const currentBL = (drafts.blacklists[langKey] || '').trim();
  const entries = currentBL
    .split(/[,;\n\r]+/)
    .map((e) => e.trim().toLowerCase())
    .filter(Boolean);

  if (!entries.includes(trimmed.toLowerCase())) {
    const updatedBL = currentBL ? `${currentBL}, ${trimmed}` : trimmed;
    drafts.blacklists[langKey] = updatedBL;
    if (elements.inputBlacklist) {
      elements.inputBlacklist.value = updatedBL;
    }
  }

  markFeedItemsBlocked(trimmed);
  if (itemElement) {
    itemElement.classList.add('feed-item-blocked');
  }

  await queueConfigSave(async () => {
    const newConfig = collectFormData();
    try {
      const saved = await invoke('save_config', { config: newConfig, base: savedConfig });
      adoptSavedConfig(newConfig, saved);
      appendLogEntry('info', `Frase añadida a lista negra: "${trimmed}"`, 'BLACKLIST');
      showToast(t('toast_added_to_blacklist'));
    } catch (err) {
      appendLogEntry('error', `Error guardando lista negra: ${err}`, 'BLACKLIST');
      showToast(`${t('status_error')}: ${err}`, true);
    }
  });
}

async function addTermToInitialPrompt(text) {
  const trimmed = text.trim();
  if (!trimmed) return;

  const langKey = currentAsrLang === 'en' ? 'en' : 'es';
  if (elements.promptEs) {
    drafts.prompts[langKey] = elements.promptEs.value;
  }
  const currentPrompt = (drafts.prompts[langKey] || '').trim();
  const entries = currentPrompt
    .split(/[,;\n\r]+/)
    .map((e) => e.trim().toLowerCase())
    .filter(Boolean);

  if (!entries.includes(trimmed.toLowerCase())) {
    const updatedPrompt = currentPrompt ? `${currentPrompt}, ${trimmed}` : trimmed;
    drafts.prompts[langKey] = updatedPrompt;
    if (elements.promptEs) {
      elements.promptEs.value = updatedPrompt;
    }
  }

  await queueConfigSave(async () => {
    const newConfig = collectFormData();
    try {
      const saved = await invoke('save_config', { config: newConfig, base: savedConfig });
      adoptSavedConfig(newConfig, saved);
      appendLogEntry('info', `Término añadido a vocabulario/prompt: "${trimmed}"`, 'PROMPT');
      showToast(t('toast_added_to_prompt'));
    } catch (err) {
      appendLogEntry('error', `Error guardando vocabulario/prompt: ${err}`, 'PROMPT');
      showToast(`${t('status_error')}: ${err}`, true);
    }
  });
}

function copyTextFallback(text) {
  const ta = document.createElement('textarea');
  ta.value = text;
  ta.style.position = 'fixed';
  ta.style.opacity = '0';
  ta.style.pointerEvents = 'none';
  document.body.appendChild(ta);
  ta.focus();
  ta.select();
  const successful = document.execCommand('copy');
  ta.remove();
  if (!successful) throw new Error('execCommand copy failed');
}

async function copyTextToClipboard(text, toastKey = 'toast_text_copied') {
  const trimmed = text.trim();
  if (!trimmed) return;
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(trimmed);
    } else {
      copyTextFallback(trimmed);
    }
    showToast(t(toastKey));
  } catch (err) {
    try {
      copyTextFallback(trimmed);
      showToast(t(toastKey));
    } catch (fallbackErr) {
      showToast(`Error al copiar: ${err}`, true);
    }
  }
}

function clearSubtitleFeed() {
  if (!elements.subtitlesFeedContainer) return;
  closeFeedActionMenu();
  const items = elements.subtitlesFeedContainer.querySelectorAll('.feed-item');
  items.forEach((item) => item.remove());
  if (elements.feedEmptyState) {
    const isLive = Boolean(currentStatus?.is_running || currentStatus?.monitor_mode || (monitorFeed && monitorFeed.socket));
    elements.feedEmptyState.textContent = isLive
      ? (t('feed_listening_live') || 'Escuchando audio en vivo — habla para ver los subtítulos en tiempo real...')
      : (t('feed_empty') || 'Los subtítulos transcritos aparecerán aquí...');
    elements.feedEmptyState.hidden = false;
  }
  if (elements.feedCountBadge) {
    elements.feedCountBadge.textContent = '0';
  }
  showToast(t('toast_feed_cleared'));
}

function renderSubtitle(text, style, cue = null) {
  if (!text || text === t('sample_preview_text')) return;
  addSubtitleToFeed(text, cue || { text, style, timestamp_ms: Date.now() });
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

async function openLicenseLink(event) {
  if (!isTauri) return;
  event.preventDefault();
  const url = event.currentTarget.href;
  try {
    await invoke('open_nvidia_license_url', { url });
  } catch (err) {
    showToast(`${t('toast_author_profile_open_failed')}${err}`, true);
  }
}

function licenseLinkLabel(url) {
  if (url.includes('libcublas')) return t('nvidia_license_cublas');
  if (url.includes('cudnn')) return t('nvidia_license_cudnn');
  try { return new URL(url).hostname; } catch (_) { return url; }
}

// About modal: local usage stats and fixed-destination support links
function formatAudioDuration(seconds) {
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return seconds > 0 ? t('about_lt_minute') : '0 min';
  const hours = Math.floor(minutes / 60);
  return hours > 0 ? `${hours} h ${minutes % 60} min` : `${minutes} min`;
}

async function openSupportLink(id) {
  try {
    await invoke('open_support_link', { id });
  } catch (err) {
    showToast(`${t('toast_support_open_failed')}${err}`, true);
  }
}

let aboutReturnFocus = null;

function closeAbout() {
  elements.aboutModal.hidden = true;
  document.removeEventListener('keydown', onAboutKeyDown);
  aboutReturnFocus?.focus();
}

function onAboutKeyDown(e) {
  if (e.key === 'Escape') {
    e.preventDefault();
    closeAbout();
  } else if (e.key === 'Tab') {
    const items = [...elements.aboutModal.querySelectorAll('button')];
    const first = items[0];
    const last = items[items.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }
}

async function openAbout() {
  let stats = { audio_seconds: 0, subtitles: 0, sessions: 0, first_used: 0 };
  try {
    stats = await invoke('get_usage_stats') || stats;
  } catch (_) { /* show the empty state */ }
  elements.aboutStatTime.textContent = formatAudioDuration(stats.audio_seconds);
  elements.aboutStatSubtitles.textContent = String(stats.subtitles);
  elements.aboutStatSessions.textContent = String(stats.sessions);
  const used = stats.subtitles > 0 || stats.sessions > 0 || stats.audio_seconds > 0;
  elements.aboutEmpty.hidden = used;
  elements.aboutSince.textContent = used && stats.first_used
    ? t('about_since').replace('{date}', new Date(stats.first_used * 1000).toLocaleDateString(getLanguage()))
    : '';
  aboutReturnFocus = document.activeElement;
  elements.aboutModal.hidden = false;
  document.addEventListener('keydown', onAboutKeyDown);
  elements.btnAboutClose.focus();
}

async function reportProblem() {
  try {
    const environment = await invoke('get_support_environment');
    await copyTextToClipboard(environment, 'about_report_copied');
  } catch (_) { /* the report page still opens without the copied details */ }
  await openSupportLink('issues');
}

// Confirm-modal icons: whitelisted keys mapped to hardcoded Lucide (ISC) markup. Callers pass keys, never markup.
const MODAL_ICONS = {
  warning: '<svg class="lucide lucide-triangle-alert" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/></svg>',
  refresh: '<svg class="lucide lucide-refresh-cw" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></svg>',
  download: '<svg class="lucide lucide-download" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" x2="12" y1="15" y2="3"/></svg>',
};

// Accessible Modal Dialog System (replaces window.alert and window.confirm)
function showConfirmModal({
  title = t('modal_title_confirm'),
  message = '',
  detail = '',
  links = [],
  confirmText = t('modal_confirm'),
  cancelText = t('modal_cancel'),
  isDanger = false,
  icon = 'warning',
} = {}) {
  return new Promise((resolve) => {
    const container = document.getElementById('modal-container');
    const titleEl = document.getElementById('modal-title');
    const msgEl = document.getElementById('modal-message');
    const detailEl = document.getElementById('modal-detail');
    const iconEl = document.getElementById('modal-icon');
    const btnConfirm = document.getElementById('modal-btn-confirm');
    const btnCancel = document.getElementById('modal-btn-cancel');
    const btnClose = document.getElementById('modal-btn-close');

    if (!container || !btnConfirm || !btnCancel) {
      resolve(window.confirm(message));
      return;
    }

    titleEl.textContent = title;
    msgEl.textContent = message;
    if (detail) {
      detailEl.textContent = detail;
      detailEl.hidden = false;
    } else {
      detailEl.hidden = true;
    }
    const linksEl = document.getElementById('modal-links');
    if (linksEl) {
      linksEl.replaceChildren(...links.map((url) => {
        const a = document.createElement('a');
        a.href = url;
        a.textContent = licenseLinkLabel(url);
        a.title = url;
        a.className = 'modal-license-link';
        a.target = '_blank';
        a.rel = 'noopener noreferrer';
        a.addEventListener('click', openLicenseLink);
        return a;
      }));
      linksEl.hidden = links.length === 0;
    }
    iconEl.innerHTML = MODAL_ICONS[Object.hasOwn(MODAL_ICONS, icon) ? icon : 'warning'];
    btnConfirm.textContent = confirmText;
    btnCancel.textContent = cancelText;

    if (isDanger) {
      btnConfirm.classList.add('btn-danger');
    } else {
      btnConfirm.classList.remove('btn-danger');
    }

    const cleanup = () => {
      container.hidden = true;
      document.removeEventListener('keydown', onKeyDown);
      btnConfirm.removeEventListener('click', onConfirm);
      btnCancel.removeEventListener('click', onCancel);
      if (btnClose) btnClose.removeEventListener('click', onCancel);
      container.removeEventListener('click', onBackdropClick);
    };

    const onConfirm = () => {
      cleanup();
      resolve(true);
    };

    const onCancel = () => {
      cleanup();
      resolve(false);
    };

    const onBackdropClick = (e) => {
      if (e.target === container) {
        onCancel();
      }
    };

    const onKeyDown = (e) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onCancel();
      } else if (e.key === 'Enter') {
        e.preventDefault();
        onConfirm();
      }
    };

    btnConfirm.addEventListener('click', onConfirm);
    btnCancel.addEventListener('click', onCancel);
    if (btnClose) btnClose.addEventListener('click', onCancel);
    container.addEventListener('click', onBackdropClick);
    document.addEventListener('keydown', onKeyDown);

    container.hidden = false;
    btnConfirm.focus();
  });
}

// 3rd Loading State for Master Start/Stop Toggle Button
function setServiceButtonLoading(mode) {
  elements.btnToggleService.className = 'btn-toggle-service loading';
  elements.btnToggleService.disabled = true;
  const label = mode === 'starting' ? t('btn_starting') : t('btn_stopping');
  elements.btnToggleService.innerHTML = `
    <svg class="spinner" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
      <circle cx="12" cy="12" r="10" stroke-opacity="0.25"></circle>
      <path d="M12 2a10 10 0 0 1 10 10" stroke-linecap="round"></path>
    </svg>
    <span id="btn-toggle-service-text">${label}</span>
  `;
}

// Adopt a config changed outside this window (for example by the MCP server). Unsaved form edits
// are kept; the save merges them with the file, so external values survive either way.
function handleExternalConfigChange({ config, changed_keys: changedKeys = [], restart_required: restartRequired }) {
  const hasUnsavedEdits = savedConfig !== null && JSON.stringify(generalFields(collectFormData())) !== JSON.stringify(generalFields(savedConfig));
  const update = externalConfigUpdate({ hasUnsavedEdits, restartRequired, isRunning: currentStatus.is_running });
  if (update.refreshForm) {
    // While the Style tab has an unsent edit, the form keeps the user's style; its save sends that edit.
    populateForm(styleDirty ? { ...config, ...styleFields(currentConfig) } : config);
    if (savedConfig) savedConfig = adoptSavedStyle(savedConfig, config);
    renderStyleTab(currentConfig);
  } else if (changedKeys.some((key) => STYLE_CONFIG_KEYS.includes(key))) {
    refreshStyleTab(config);
  }
  showToast(t(update.messageKey));
}

// The style fields are owned by save_subtitle_style, so an external change to them is adopted on its
// own: general form edits that are still unsaved stay in the form. An unsent Style edit wins over the
// file; only the saved baseline takes the external value, and the pending save replaces the file.
function refreshStyleTab(config) {
  if (savedConfig) savedConfig = adoptSavedStyle(savedConfig, config);
  if (styleDirty) return;
  currentConfig = adoptSavedStyle(currentConfig, config);
  renderStyleTab(config);
}

// Adopts the config the backend saved. Only fields the user has not changed since the save was sent
// take the backend value; the rest of the form (typed during the save) is left as the user left it.
// Style fields are never merged here (see mergeSavedConfigIntoForm).
function adoptSavedConfig(sent, saved) {
  populateForm(mergeSavedConfigIntoForm(sent, collectFormData(), saved), false);
  savedConfig = structuredClone(saved);
  syncAdvancedUnsaved();
}

/**
 * Renders the style controls and presets from a config. Unless `force` is set, a style edit the user
 * has not yet saved keeps its value on screen: the pending save sends it.
 */
function renderStyleTab(config, { force = false } = {}) {
  if (!force && styleDirty) return;
  renderStyleSelector(config);
  populateStyleControls(controlsFromTheme(config.subtitle_theme || {}));
}

/** Built-in styles plus the saved ones under "My styles"; a saved style stays selected while it exists. */
function renderStyleSelector(config) {
  const presets = config.subtitle_presets || [];
  elements.optgroupSubPresets.label = t('lbl_sub_my_styles');
  elements.optgroupSubPresets.hidden = presets.length === 0;
  elements.optgroupSubPresets.replaceChildren(...presets.map((preset) => new Option(preset.name, presetOptionValue(preset.name))));
  if (!presets.some((preset) => preset.name === activePresetName)) activePresetName = '';
  elements.selectSubtitleStyle.value = activePresetName ? presetOptionValue(activePresetName) : (config.subtitle_style || 'default');
}

/** Applies the style selector's choice; a saved style brings its base style (if any) and its tokens. */
function applyStyleChoice() {
  const name = presetNameFromOption(elements.selectSubtitleStyle.value);
  const preset = name === null ? null : (currentConfig.subtitle_presets || []).find((item) => item.name === name);
  if (name !== null && !preset) return false;
  activePresetName = preset ? preset.name : '';
  if (preset) {
    const loaded = loadPresetTheme(preset);
    currentConfig = { ...currentConfig, subtitle_theme: { ...loaded.theme }, subtitle_style: loaded.style ?? currentConfig.subtitle_style };
    populateStyleControls(loaded.controls);
    elements.inputSubPresetName.value = preset.name;
    showSubStyleStatus(t('sub_preset_loaded'));
  } else {
    currentConfig = { ...currentConfig, subtitle_style: elements.selectSubtitleStyle.value };
    renderSubtitlePreview();
  }
  return true;
}

/** The built-in style behind the selector: a saved style carries its own in the config. */
function baseStyle() {
  const value = elements.selectSubtitleStyle.value;
  return presetNameFromOption(value) === null ? value : (currentConfig?.subtitle_style || 'default');
}

// Style tab helpers
let previewAppliedProps = [];
let subtitleStyleSaveTimer = null;
// True while a Style tab edit has not been sent yet, or was made after the last style save was sent.
let styleDirty = false;
// Installed font families from the backend; empty keeps the curated platform list.
let systemFonts = [];
// Name of the saved style chosen in the style selector, '' when a built-in style is chosen.
let activePresetName = '';
let previewPlayTimer = null;

/** Fills the font picker: installed families, else the platform list; a saved font from elsewhere stays selectable. */
function renderFontOptions(selected) {
  const platformFonts = fontFamiliesForPlatform(navigator.userAgent || navigator.platform || '');
  const fonts = fontChoices(systemFonts, platformFonts, selected);
  elements.selectSubFont.replaceChildren(...fonts.map((family) => new Option(family, family)));
  elements.selectSubFont.value = selected;
}

function populateStyleControls(values) {
  renderFontOptions(values.fontFamily);
  elements.inputSubFontSize.value = values.fontSize;
  elements.selectSubWeight.value = values.fontWeight;
  elements.selectSubAnimation.value = values.animation;
  elements.inputSubAnimDuration.value = values.animationDuration;
  elements.inputSubLifetime.value = values.lifetime;
  elements.inputSubLetterSpacing.value = values.letterSpacing;
  syncLetterSpacingReadout();
  elements.inputSubWordSpacing.value = values.wordSpacing;
  syncWordSpacingReadout();
  elements.inputSubColor.value = values.color;
  elements.inputSubBgCustom.checked = values.background !== null;
  elements.inputSubBg.value = values.background ?? '#000000';
  elements.inputSubBgOpacity.value = Math.round(values.backgroundOpacity * 100);
  syncBackgroundControls();
  renderSubtitlePreview();
}

/** The background picker and opacity are only usable when a custom background is on. */
function syncBackgroundControls() {
  const custom = elements.inputSubBgCustom.checked;
  elements.inputSubBg.disabled = !custom;
  elements.inputSubBgOpacity.disabled = !custom;
}

function currentSubtitleTheme() {
  return themeFromControls({
    fontFamily: elements.selectSubFont.value,
    fontSize: Number(elements.inputSubFontSize.value),
    fontWeight: elements.selectSubWeight.value,
    color: elements.inputSubColor.value,
    background: elements.inputSubBgCustom.checked ? elements.inputSubBg.value : null,
    backgroundOpacity: Number(elements.inputSubBgOpacity.value) / 100,
    animation: elements.selectSubAnimation.value,
    animationDuration: Number(elements.inputSubAnimDuration.value),
    lifetime: Number(elements.inputSubLifetime.value),
    letterSpacing: Number(elements.inputSubLetterSpacing.value),
    wordSpacing: Number(elements.inputSubWordSpacing.value),
  }, currentConfig?.subtitle_theme ?? {});
}

function syncLetterSpacingReadout() {
  elements.valSubLetterSpacing.textContent = `${elements.inputSubLetterSpacing.value}px`;
}

function syncWordSpacingReadout() {
  elements.valSubWordSpacing.textContent = `${elements.inputSubWordSpacing.value}px`;
}

const sortedTokens = (theme) => JSON.stringify(Object.entries(theme).sort(([a], [b]) => (a < b ? -1 : 1)));

/**
 * Style edits save to the live theme on their own; what can be left unsaved is a change to a saved
 * style ("My styles"), which only the Save button writes back. True while the controls differ from it.
 */
function styleChangesUnsaved() {
  const preset = activePresetName ? (currentConfig?.subtitle_presets || []).find((item) => item.name === activePresetName) : null;
  if (!preset) return false;
  const saved = loadPresetTheme(preset);
  return sortedTokens(currentSubtitleTheme()) !== sortedTokens(themeFromControls(saved.controls, saved.theme));
}

function syncUnsavedStyleChip() {
  if (elements.chipSubStyleUnsaved) elements.chipSubStyleUnsaved.hidden = !styleChangesUnsaved();
}

/** Writes the current controls into the saved style `name` and selects it. Returns false on a name or limit error. */
function applyPresetSave(name) {
  const result = savePreset(currentConfig.subtitle_presets || [], name, currentSubtitleTheme(), currentConfig.subtitle_style);
  if (result.error) {
    showSubStyleStatus(t(`sub_preset_err_${result.error}`), true);
    return false;
  }
  currentConfig = { ...currentConfig, subtitle_presets: result.presets };
  activePresetName = name;
  renderStyleSelector(currentConfig);
  syncUnsavedStyleChip();
  showSubStyleStatus(t('sub_preset_saved'));
  scheduleSubtitleStyleSave(0);
  return true;
}

/** Asks Save or Discard when leaving the Style tab (or closing) with unsaved saved-style edits; always lets the user go on. */
async function confirmLeaveStyleTab() {
  if (!styleChangesUnsaved()) return true;
  const name = activePresetName;
  const save = await showConfirmModal({
    title: t('sub_unsaved_title'),
    message: t('sub_unsaved_message').replace('{name}', name),
    confirmText: t('btn_sub_unsaved_save'),
    cancelText: t('btn_sub_unsaved_discard'),
  });
  if (save) {
    applyPresetSave(name);
  } else {
    const loaded = loadPresetTheme((currentConfig.subtitle_presets || []).find((item) => item.name === name));
    currentConfig = { ...currentConfig, subtitle_theme: { ...loaded.theme } };
    populateStyleControls(loaded.controls);
    scheduleSubtitleStyleSave(0);
  }
  return true;
}

/**
 * Advanced settings are not auto-saved: they go out with the footer Save button, like the General tab.
 * True while the Advanced controls differ from the last saved config.
 */
function advancedChangesUnsaved() {
  if (!savedConfig) return false;
  return JSON.stringify(advancedView(collectFormData())) !== JSON.stringify(advancedView(savedConfig));
}

function syncAdvancedUnsaved() {
  if (elements.chipAdvUnsaved) elements.chipAdvUnsaved.hidden = !advancedChangesUnsaved();
}

/** Asks Save or Discard when leaving the Advanced tab (or closing) with unsaved changes. False keeps the user here. */
async function confirmLeaveAdvancedTab() {
  if (!advancedChangesUnsaved()) return true;
  const save = await showConfirmModal({
    title: t('adv_unsaved_title'),
    message: t('adv_unsaved_message'),
    confirmText: t('btn_adv_unsaved_save'),
    cancelText: t('btn_adv_unsaved_discard'),
  });
  if (save) {
    await saveGeneralConfig();
    syncAdvancedUnsaved();
    return !advancedChangesUnsaved();
  }
  populateAdvancedControls(savedConfig);
  syncAdvancedUnsaved();
  return true;
}

let activeTabId = 'tab-setup';

/** The footer Reset belongs to the active tab; tabs with nothing to reset (Home, Style) hide it. */
function setActiveTab(id) {
  activeTabId = id;
  if (elements.btnResetConfig) elements.btnResetConfig.hidden = !TAB_RESETS[id];
}

/** Puts only the active tab's controls back to their defaults, in the form. Nothing is saved until the user presses Save. */
async function resetActiveTab() {
  const tab = TAB_RESETS[activeTabId];
  if (!tab) return;
  const ok = await showConfirmModal({
    title: t('tab_reset_title').replace('{tab}', t(tab.title)),
    message: t('tab_reset_message').replace('{list}', tab.labels.map((key) => t(key)).join(', ')),
    confirmText: t('tab_reset_confirm'),
    isDanger: true,
    icon: 'refresh',
  });
  if (!ok) return;
  const d = tab.defaults(navigator.hardwareConcurrency || 8);
  if (activeTabId === 'tab-advanced') {
    populateAdvancedControls({ ...currentConfig, ...d });
    syncAdvancedUnsaved();
  } else if (activeTabId === 'tab-subtitles') {
    elements.selectDisplayMode.value = d.subtitle_display_mode;
    elements.sliderRibbonLines.value = d.subtitle_ribbon_max_lines;
    elements.valRibbonLines.textContent = d.subtitle_ribbon_max_lines;
  } else {
    elements.checkContinuous.checked = d.continuous_session;
    elements.checkSaveTranscript.checked = d.save_transcript_enabled;
    elements.checkSaveVtt.checked = d.save_vtt_enabled;
  }
  showToast(t('tab_reset_done'));
}

/** Back to the built-in defaults of the selected style: clears the theme tokens, keeps the style choice. */
async function resetSubtitleStyle() {
  const ok = await showConfirmModal({
    title: t('sub_reset_title'),
    message: t('sub_reset_message'),
    confirmText: t('sub_reset_confirm'),
    icon: 'refresh',
  });
  if (!ok) return;
  currentConfig = { ...currentConfig, subtitle_theme: {} };
  populateStyleControls(controlsFromTheme({}));
  showSubStyleStatus(t('sub_style_reset_done'));
  scheduleSubtitleStyleSave(0);
}

function renderSubtitlePreview() {
  clearTimeout(previewPlayTimer); // an edit ends a Play in progress
  syncUnsavedStyleChip();
  // Same rules as the overlay: shared CSS (overlay-preview.css) plus the properties applyTheme sets.
  elements.subStylePreviewText.className = `sub-box visible style-${baseStyle()}`;
  const { props, anim, font } = previewProperties(currentSubtitleTheme());
  for (const name of previewAppliedProps) elements.subStylePreview.style.removeProperty(name);
  for (const [name, value] of Object.entries(props)) elements.subStylePreview.style.setProperty(name, value);
  previewAppliedProps = Object.keys(props);
  elements.subStylePreview.dataset.anim = anim;
  elements.subStylePreview.dataset.font = font;

  const style = baseStyle();
  const rawText = t('sub_style_preview_text') || 'Así se verán tus subtítulos';
  const cleanText = String(rawText).replace(/\s+/g, ' ').trim();
  if (style === 'karaoke' || style === 'rgb' || style === 'typewriter') {
    elements.subStylePreviewText.textContent = '';
    const words = cleanText.split(' ').filter(Boolean);
    words.forEach((word, index) => {
      const span = document.createElement('span');
      span.className = 'word';
      span.textContent = word;
      if (style === 'rgb') {
        const hue = Math.floor((index / Math.max(words.length, 1)) * 360);
        span.style.color = `hsl(${hue}, 85%, 60%)`;
      }
      elements.subStylePreviewText.appendChild(span);
      if (index < words.length - 1) {
        elements.subStylePreviewText.appendChild(document.createTextNode(' '));
      }
    });
  } else {
    elements.subStylePreviewText.textContent = cleanText;
  }
}

/**
 * Replays the overlay's life of a line on the preview: enter, word reveal, hold, exit, then the static
 * preview again. The hold is the time on screen, capped at 3 s. Reduced motion keeps the preview static.
 */
function playSubtitlePreview() {
  renderSubtitlePreview();
  if (window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches) return;
  const line = elements.subStylePreviewText;
  const theme = currentSubtitleTheme();
  const { animationDuration, lifetime } = controlsFromTheme(theme);
  const enterMs = theme['--sub-animation'] === 'none' ? 0 : animationDuration * 1000;
  const words = line.querySelectorAll('.word');
  previewWordDelays(baseStyle(), words.length).forEach((delay, index) => { words[index].style.animationDelay = `${delay}ms`; });
  line.classList.remove('visible');
  void line.offsetHeight; // flush the hidden state so the enter transition runs
  previewPlayTimer = setTimeout(() => {
    line.classList.add('visible');
    previewPlayTimer = setTimeout(() => {
      line.classList.remove('visible');
      line.classList.add('hiding');
      previewPlayTimer = setTimeout(renderSubtitlePreview, Math.max(650, enterMs + 100));
    }, enterMs + previewHoldMs(lifetime));
  }, 20);
}

let subStyleStatusIsError = false;

function showSubStyleStatus(message, isError = false) {
  subStyleStatusIsError = isError;
  elements.subStyleStatus.textContent = message;
  elements.subStyleStatus.classList.toggle('error-text', isError);
}

function scheduleSubtitleStyleSave(delayMs = 250) {
  styleDirty = true;
  clearTimeout(subtitleStyleSaveTimer);
  // Shares the config save queue, so a style save never interleaves with a general save.
  subtitleStyleSaveTimer = setTimeout(() => queueConfigSave(saveSubtitleStyle), delayMs);
}

async function saveSubtitleStyle() {
  subtitleStyleSaveTimer = null;
  const theme = currentSubtitleTheme();
  const presets = currentConfig.subtitle_presets || [];
  styleDirty = false;
  try {
    const updated = await invoke('save_subtitle_style', { theme, presets, subtitleStyle: currentConfig.subtitle_style });
    currentConfig = adoptSavedStyle(currentConfig, updated);
    if (savedConfig) savedConfig = adoptSavedStyle(savedConfig, updated);
    renderStyleTab(updated);
    // Clears only an earlier error: a preset message ("saved", "loaded", "deleted") stays visible.
    if (subStyleStatusIsError) showSubStyleStatus('');
  } catch (error) {
    styleDirty = true; // the edits were not saved: keep them on screen, and the next edit or save retries
    showSubStyleStatus(t('sub_style_save_failed'), true);
    showToast(`${t('sub_style_save_failed')}: ${error}`, true);
  }
}

/** Fills the Advanced tab controls from a config (also used to discard or reset them). */
function populateAdvancedControls(config) {
  // Audio & Hardware compute device
  if (elements.selectComputeDevice) {
    elements.selectComputeDevice.value = config.device || 'cpu';
  } else if (config.device === 'cuda') {
    if (elements.deviceRadioCuda) elements.deviceRadioCuda.checked = true;
  } else {
    if (elements.deviceRadioCpu) elements.deviceRadioCpu.checked = true;
  }
  updateComputeDeviceReactivity(config.device || 'cpu');

  elements.sliderCpuThreads.value = config.cpu_threads;
  elements.valCpuThreads.textContent = config.cpu_threads;

  elements.sliderSilenceTimeout.value = config.silence_timeout;
  elements.valSilenceTimeout.textContent = `${config.silence_timeout}s`;

  elements.sliderMaxChunk.value = config.max_chunk_duration;
  elements.valMaxChunk.textContent = `${config.max_chunk_duration}s`;

  // Model size
  for (let opt of elements.selectModelSize.options) {
    if (opt.value.startsWith(config.model_size.split(' ')[0])) {
      opt.selected = true;
      break;
    }
  }

  elements.selectBacklogPolicy.value = config.subtitle_backlog_policy || 'auto';
  elements.selectAsrBackend.value = config.asr_backend || 'auto';

  elements.sliderMaxLiveDelay.value = config.subtitle_max_live_delay_sec || 10.0;
  elements.valMaxLiveDelay.textContent = `${config.subtitle_max_live_delay_sec || 10.0}s`;

  elements.inputWsPort.value = config.ws_port || 8765;
  elements.sliderVadPad.value = config.vad_speech_pad_ms || 200;
  elements.valVadPad.textContent = `${config.vad_speech_pad_ms || 200}ms`;

  elements.sliderVadThreshold.value = config.vad_threshold || 0.5;
  elements.valVadThreshold.textContent = `${config.vad_threshold || 0.5}`;

  elements.checkRnnoiseEnabled.checked = Boolean(config.rnnoise_enabled);
  if (elements.checkMcpStyleEnabled) elements.checkMcpStyleEnabled.checked = config.mcp_style_enabled === true;
  if (elements.checkMcpSystemEnabled) elements.checkMcpSystemEnabled.checked = config.mcp_system_enabled === true;
  if (elements.checkUpdatesEnabled) elements.checkUpdatesEnabled.checked = config.check_updates_enabled !== false;
  elements.checkAcousticEventsEnabled.checked = Boolean(config.acoustic_events_enabled);
  if (elements.checkLivePartialSubtitles) elements.checkLivePartialSubtitles.checked = Boolean(config.live_partial_subtitles_enabled);
  elements.sliderCatchupInterval.value = config.subtitle_catchup_interval_sec || 1.5;
  elements.valCatchupInterval.textContent = `${config.subtitle_catchup_interval_sec || 1.5}s`;
}

// Populate Settings Form from Config
function populateForm(config, updateSavedBaseline = true) {
  currentConfig = config;
  if (updateSavedBaseline) savedConfig = structuredClone(config);

  // Sync UI Language
  if (config.language && (config.language === 'es' || config.language === 'en')) {
    setLanguage(config.language);
    if (elements.selectUiLang) {
      elements.selectUiLang.value = config.language;
    }
  }

  currentAsrLang = config.asr_language || 'es';
  drafts.prompts.es = config.whisper_context_prompt_es || '';
  drafts.prompts.en = config.whisper_context_prompt_en || '';
  drafts.blacklists.es = config.blacklist || drafts.blacklists.es;
  drafts.blacklists.en = (config.extra && (config.extra.blacklist_en || config.extra.blacklist)) || drafts.blacklists.en;

  elements.selectAsrLang.value = currentAsrLang;
  syncAsrLanguageUi(currentAsrLang);

  // Subtitles Tab
  renderStyleSelector(config);
  elements.selectDisplayMode.value = config.subtitle_display_mode || 'adaptive';

  elements.sliderRibbonLines.value = config.subtitle_ribbon_max_lines || 3;
  elements.valRibbonLines.textContent = config.subtitle_ribbon_max_lines || 3;

  populateAdvancedControls(config);

  // Files tab and profile
  elements.checkContinuous.checked = Boolean(config.continuous_session);
  elements.checkSaveTranscript.checked = Boolean(config.save_transcript_enabled);
  elements.checkSaveVtt.checked = Boolean(config.save_vtt_enabled);
  elements.selectProfile.value = config.profile_mode === 'custom' ? 'custom' : (config.selected_profile_id || 'balanced');

  updateProfileDescription();
  syncAdvancedUnsaved();
}

// Synchronize dynamic prompt and blacklist content based on active spoken language
function syncAsrLanguageUi(newLang) {
  if (newLang !== currentAsrLang) {
    if (elements.promptEs) {
      drafts.prompts[currentAsrLang] = elements.promptEs.value;
    }
    if (elements.inputBlacklist) {
      drafts.blacklists[currentAsrLang] = elements.inputBlacklist.value;
    }
    currentAsrLang = newLang;
  }

  if (elements.badgePromptLang) {
    elements.badgePromptLang.textContent = newLang === 'en' ? '🇺🇸 EN' : '🇪🇸 ES';
  }
  if (elements.badgeBlacklistLang) {
    elements.badgeBlacklistLang.textContent = newLang === 'en' ? '🇺🇸 EN' : '🇪🇸 ES';
  }

  if (elements.promptEs) {
    elements.promptEs.value = drafts.prompts[newLang] || '';
    elements.promptEs.placeholder = newLang === 'en'
      ? 'Custom gaming terms, names, slang...'
      : 'Términos técnicos, jerga de juego, nombres propios...';
  }
  if (elements.hintPrompt) {
    elements.hintPrompt.textContent = newLang === 'en'
      ? "Ayuda a reducir alucinaciones en inglés. Ej: 'Coding stream, Python tutorials, backend development'."
      : "Ayuda a reducir alucinaciones en español. Ej: 'Stream de gaming, jugamos Minecraft y charlamos con viewers'.";
  }

  if (elements.inputBlacklist) {
    elements.inputBlacklist.value = drafts.blacklists[newLang] || '';
  }
  if (elements.hintBlacklist) {
    elements.hintBlacklist.textContent = newLang === 'en'
      ? "Palabras en inglés que se omitirán si Whisper las genera en silencio."
      : "Palabras separadas por comas que se omitirán si Whisper las genera en silencio.";
  }
}

// Collect Modified Form Data into Config
function collectFormData() {
  const cfg = { ...currentConfig };

  // Save active inputs into current language draft
  if (elements.promptEs) {
    drafts.prompts[currentAsrLang] = elements.promptEs.value;
  }
  if (elements.inputBlacklist) {
    drafts.blacklists[currentAsrLang] = elements.inputBlacklist.value;
  }

  cfg.language = getLanguage();
  cfg.device = elements.selectComputeDevice
    ? elements.selectComputeDevice.value
    : (elements.deviceRadioCuda && elements.deviceRadioCuda.checked ? 'cuda' : 'cpu');
  cfg.cpu_threads = parseInt(elements.sliderCpuThreads.value, 10);
  cfg.silence_timeout = parseFloat(elements.sliderSilenceTimeout.value);
  cfg.max_chunk_duration = parseFloat(elements.sliderMaxChunk.value);
  if (cfg.max_chunk_duration > 15.0) {
    cfg.transcription_purpose = 'transcription';
  }

  cfg.model_size = elements.selectModelSize.value;
  cfg.asr_language = currentAsrLang;
  cfg.whisper_context_prompt_es = drafts.prompts.es.trim();
  cfg.whisper_context_prompt_en = drafts.prompts.en.trim();
  cfg.blacklist = drafts.blacklists[currentAsrLang].trim();
  if (!cfg.extra) cfg.extra = {};
  cfg.extra.blacklist_es = drafts.blacklists.es.trim();
  cfg.extra.blacklist_en = drafts.blacklists.en.trim();

  cfg.subtitle_style = currentConfig?.subtitle_style || 'default';
  cfg.subtitle_theme = currentConfig?.subtitle_theme || {};
  cfg.subtitle_presets = currentConfig?.subtitle_presets || [];
  cfg.subtitle_display_mode = elements.selectDisplayMode.value;
  cfg.subtitle_ribbon_max_lines = parseInt(elements.sliderRibbonLines.value, 10);
  cfg.subtitle_backlog_policy = elements.selectBacklogPolicy.value;
  cfg.asr_backend = elements.selectAsrBackend.value;
  cfg.subtitle_max_live_delay_sec = parseFloat(elements.sliderMaxLiveDelay.value);
  cfg.subtitle_catchup_interval_sec = parseFloat(elements.sliderCatchupInterval.value);

  cfg.ws_port = parseInt(elements.inputWsPort.value, 10) || 8765;
  cfg.vad_speech_pad_ms = parseInt(elements.sliderVadPad.value, 10);
  cfg.vad_threshold = parseFloat(elements.sliderVadThreshold.value);
  cfg.continuous_session = elements.checkContinuous.checked;
  cfg.save_transcript_enabled = elements.checkSaveTranscript.checked;
  cfg.save_vtt_enabled = elements.checkSaveVtt.checked;
  cfg.rnnoise_enabled = elements.checkRnnoiseEnabled.checked;
  if (elements.checkMcpStyleEnabled) cfg.mcp_style_enabled = elements.checkMcpStyleEnabled.checked;
  if (elements.checkMcpSystemEnabled) cfg.mcp_system_enabled = elements.checkMcpSystemEnabled.checked;
  if (elements.checkUpdatesEnabled) cfg.check_updates_enabled = elements.checkUpdatesEnabled.checked;
  cfg.acoustic_events_enabled = elements.checkAcousticEventsEnabled.checked;
  if (elements.checkLivePartialSubtitles) cfg.live_partial_subtitles_enabled = elements.checkLivePartialSubtitles.checked;
  cfg.selected_profile_id = elements.selectProfile.value;
  cfg.profile_mode = elements.selectProfile.value === 'custom' ? 'custom' : 'preset';
  const selectedPreset = profilePresets.find((profile) => profile.id === elements.selectProfile.value);
  if (selectedPreset && (
    cfg.device !== selectedPreset.device
    || cfg.model_size !== selectedPreset.model_size
    || cfg.silence_timeout !== selectedPreset.silence_timeout
    || cfg.subtitle_backlog_policy !== selectedPreset.subtitle_backlog_policy
    || cfg.subtitle_max_live_delay_sec !== selectedPreset.subtitle_max_live_delay_sec
    || cfg.subtitle_catchup_interval_sec !== selectedPreset.subtitle_catchup_interval_sec
  )) {
    cfg.selected_profile_id = 'custom';
    cfg.profile_mode = 'custom';
  }

  const selectedDeviceOption = elements.audioDeviceSelect.options[elements.audioDeviceSelect.selectedIndex];
  if (selectedDeviceOption && selectedDeviceOption.value) {
    cfg.audio_device = {
      id: selectedDeviceOption.value,
      name: selectedDeviceOption.text,
    };
  }

  return cfg;
}

// Populate Devices Dropdown
async function loadAudioDevices() {
  try {
    const devices = await invoke('get_audio_devices');
    elements.audioDeviceSelect.innerHTML = '';

    devices.forEach((dev) => {
      const opt = document.createElement('option');
      opt.value = dev.id;
      const defaultSuffix = dev.is_default ? ` (${getLanguage() === 'es' ? 'Predeterminado' : 'Default'})` : '';
      opt.textContent = `${dev.name}${defaultSuffix}`;
      if (currentConfig && currentConfig.audio_device && currentConfig.audio_device.id === dev.id) {
        opt.selected = true;
      }
      elements.audioDeviceSelect.appendChild(opt);
    });
  } catch (err) {
    console.error('Error loading devices:', err);
  }
}

async function loadProfilePresets() {
  try {
    profilePresets = await invoke('get_profile_presets');
  } catch (err) {
    console.warn('Error fetching profile presets:', err);
  }
  updateProfileOptions();
  updateProfileDescription();
}

function updateProfileOptions() {
  const lang = getLanguage();
  for (const opt of elements.selectProfile.options) {
    const data = getProfilePresetData(opt.value, lang);
    if (data && data.label) {
      opt.textContent = data.label;
    }
  }
}

function updateProfileDescription() {
  const profileId = elements.selectProfile.value;
  const data = getProfilePresetData(profileId, getLanguage());
  elements.profileDescription.textContent = data.description || t('profile_desc_custom');
}

// Load OBS Overlay Information
async function loadObsOverlayInfo() {
  try {
    const obs = await invoke('get_obs_overlay_info');
    elements.inputObsUrl.value = obs.url;
    elements.inputObsUrl.title = obs.url;
  } catch (err) {
    console.error('Error loading OBS overlay info:', err);
  }
}

// Initialize Application
let currentAllowedConnections = { allowed_ports: [1420], recent_rejected: [] };

function renderAllowedConnections(data) {
  if (!data) return;
  currentAllowedConnections = data;

  if (elements.allowedPortsList) {
    elements.allowedPortsList.innerHTML = '';
    if (!data.allowed_ports || data.allowed_ports.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'conn-empty';
      empty.textContent = t('empty_allowed_ports');
      elements.allowedPortsList.appendChild(empty);
    } else {
      for (const port of data.allowed_ports) {
        const row = document.createElement('div');
        row.className = 'conn-row';
        row.setAttribute('role', 'listitem');

        const info = document.createElement('div');
        info.className = 'conn-info';

        const portSpan = document.createElement('span');
        portSpan.className = 'conn-origin';
        portSpan.textContent = `Port ${port}`;
        info.appendChild(portSpan);

        if (port === 1420) {
          const tag = document.createElement('span');
          tag.className = 'conn-meta';
          tag.textContent = '(LiveAudio Dev)';
          info.appendChild(tag);
        }

        const revokeBtn = document.createElement('button');
        revokeBtn.type = 'button';
        revokeBtn.className = 'btn-secondary btn-sm';
        revokeBtn.textContent = t('btn_revoke_port');
        revokeBtn.setAttribute('aria-label', `${t('btn_revoke_port')} port ${port}`);
        revokeBtn.addEventListener('click', async () => {
          revokeBtn.disabled = true;
          try {
            const updated = await invoke('revoke_loopback_port', { port });
            renderAllowedConnections(updated);
          } catch (err) {
            showToast(`${t('status_error')}: ${err}`, true);
            revokeBtn.disabled = false;
          }
        });

        row.appendChild(info);
        row.appendChild(revokeBtn);
        elements.allowedPortsList.appendChild(row);
      }
    }
  }

  if (elements.recentBlockedList) {
    elements.recentBlockedList.innerHTML = '';
    if (!data.recent_rejected || data.recent_rejected.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'conn-empty';
      empty.textContent = t('empty_recent_blocked');
      elements.recentBlockedList.appendChild(empty);
    } else {
      for (const entry of data.recent_rejected) {
        const row = document.createElement('div');
        row.className = 'conn-row';
        row.setAttribute('role', 'listitem');

        const info = document.createElement('div');
        info.className = 'conn-info';

        const originSpan = document.createElement('span');
        originSpan.className = 'conn-origin';
        originSpan.textContent = entry.origin;

        const meta = document.createElement('span');
        meta.className = 'conn-meta';
        const timeStr = entry.last_attempt_timestamp_ms
          ? new Date(entry.last_attempt_timestamp_ms).toLocaleTimeString()
          : '';
        meta.textContent = `${entry.count}x · ${timeStr}`;

        info.appendChild(originSpan);
        info.appendChild(meta);
        row.appendChild(info);

        if (entry.port && !data.allowed_ports.includes(entry.port)) {
          const allowBtn = document.createElement('button');
          allowBtn.type = 'button';
          allowBtn.className = 'btn-secondary btn-sm';
          allowBtn.textContent = t('btn_allow_origin');
          allowBtn.setAttribute('aria-label', `${t('btn_allow_origin')} port ${entry.port}`);
          allowBtn.addEventListener('click', async () => {
            allowBtn.disabled = true;
            try {
              const updated = await invoke('allow_loopback_port', { port: entry.port });
              renderAllowedConnections(updated);
            } catch (err) {
              showToast(`${t('status_error')}: ${err}`, true);
              allowBtn.disabled = false;
            }
          });
          row.appendChild(allowBtn);
        }

        elements.recentBlockedList.appendChild(row);
      }
    }
  }
}

async function refreshAllowedConnections() {
  try {
    const data = await invoke('get_allowed_connections');
    renderAllowedConnections(data);
  } catch (err) {
    console.error('Error refreshing allowed connections:', err);
  }
}

function setupAllowedConnectionsHandlers() {
  if (!elements.btnAddAllowedPort || !elements.inputAddAllowedPort) return;

  const handleAdd = async () => {
    if (elements.errorAddAllowedPort) {
      elements.errorAddAllowedPort.textContent = '';
      elements.errorAddAllowedPort.hidden = true;
    }

    const rawVal = elements.inputAddAllowedPort.value.trim();
    if (!rawVal) {
      if (elements.errorAddAllowedPort) {
        elements.errorAddAllowedPort.textContent = t('err_invalid_port');
        elements.errorAddAllowedPort.hidden = false;
      }
      return;
    }

    const port = parseInt(rawVal, 10);
    if (isNaN(port) || port < 1 || port > 65535) {
      if (elements.errorAddAllowedPort) {
        elements.errorAddAllowedPort.textContent = t('err_invalid_port');
        elements.errorAddAllowedPort.hidden = false;
      }
      return;
    }

    if (currentAllowedConnections.allowed_ports && currentAllowedConnections.allowed_ports.includes(port)) {
      if (elements.errorAddAllowedPort) {
        elements.errorAddAllowedPort.textContent = t('err_port_already_allowed');
        elements.errorAddAllowedPort.hidden = false;
      }
      return;
    }

    elements.btnAddAllowedPort.disabled = true;
    try {
      const updated = await invoke('allow_loopback_port', { port });
      elements.inputAddAllowedPort.value = '';
      renderAllowedConnections(updated);
    } catch (err) {
      if (elements.errorAddAllowedPort) {
        elements.errorAddAllowedPort.textContent = String(err);
        elements.errorAddAllowedPort.hidden = false;
      }
    } finally {
      elements.btnAddAllowedPort.disabled = false;
    }
  };

  elements.btnAddAllowedPort.addEventListener('click', handleAdd);
  elements.inputAddAllowedPort.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      handleAdd();
    }
  });
}

async function initApp() {
  // Initialize bilingual i18n
  initLanguage();
  if (elements.selectUiLang) {
    elements.selectUiLang.value = getLanguage();
    elements.selectUiLang.addEventListener('change', (e) => {
      const newLang = e.target.value;
      setLanguage(newLang);
      if (currentConfig) {
        currentConfig.language = newLang;
      }
      updateProfileOptions();
      updateProfileDescription();
      updateStatusUI(currentStatus);
      applyTranslations(newLang);
      updateComputeDeviceReactivity();
    });
  }

  if (elements.selectComputeDevice) {
    elements.selectComputeDevice.addEventListener('change', (e) => {
      updateComputeDeviceReactivity(e.target.value);
    });
  }

  // Advanced controls: the chip follows every edit; Reset asks first and only edits the form.
  for (const id of ADVANCED_FIELD_IDS) {
    for (const type of ['input', 'change']) document.getElementById(id)?.addEventListener(type, syncAdvancedUnsaved);
  }
  bindTabNavigation(document.querySelector('.tab-nav'), { canLeave: (from) => (from?.id === 'tab-style-tab' ? confirmLeaveStyleTab()
    : from?.id === 'tab-advanced-tab' ? confirmLeaveAdvancedTab() : true), onActivate: (tab) => {
    setActiveTab(tab.dataset.tab);
    if (tab.dataset.tab === 'tab-advanced') refreshAllowedConnections();
  } });
  // Installed fonts for the Style tab; any failure keeps the curated list.
  invoke('list_system_fonts').then((names) => {
    if (!Array.isArray(names) || names.length === 0) return;
    systemFonts = names;
    renderFontOptions(elements.selectSubFont.value);
  }).catch(() => {});
  elements.btnNvidiaEnable.addEventListener('click', startNvidiaInstall);
  elements.btnNvidiaRetry.addEventListener('click', startNvidiaInstall);
  elements.btnNvidiaRestart.addEventListener('click', restartServiceAfterNvidia);
  elements.btnNvidiaAdvanced.addEventListener('click', () => {
    elements.tabSetupTab.click();
    startNvidiaInstall();
  });
  elements.btnNvidiaDismiss.addEventListener('click', () => {
    try { localStorage.setItem(NVIDIA_DISMISS_KEY, '1'); } catch (_) { /* storage unavailable */ }
    renderGpuStatus();
  });
  elements.btnNvidiaCancel.addEventListener('click', async () => {
    nvidiaCancelRequested = true;
    try { await invoke('cancel_nvidia_runtime_install'); } catch (_) { /* install already finished */ }
  });
  refreshGpuStatus();

  elements.authorProfileLink.addEventListener('click', async (event) => {
    if (!isTauri) return;
    event.preventDefault();
    try {
      await invoke('open_author_profile');
    } catch (err) {
      showToast(`${t('toast_author_profile_open_failed')}${err}`, true);
    }
  });

  elements.btnAbout.addEventListener('click', openAbout);
  elements.btnAboutClose.addEventListener('click', closeAbout);
  elements.aboutModal.addEventListener('click', (e) => { if (e.target === elements.aboutModal) closeAbout(); });
  elements.btnAboutReport.addEventListener('click', reportProblem);
  elements.btnAboutStar.addEventListener('click', () => openSupportLink('repo'));
  elements.btnAboutSponsor.addEventListener('click', () => openSupportLink('sponsor'));

  // Slider bindings
  elements.sliderCpuThreads.addEventListener('input', (e) => {
    elements.valCpuThreads.textContent = e.target.value;
  });
  elements.sliderSilenceTimeout.addEventListener('input', (e) => {
    elements.valSilenceTimeout.textContent = `${e.target.value}s`;
  });
  elements.sliderMaxChunk.addEventListener('input', (e) => {
    elements.valMaxChunk.textContent = `${e.target.value}s`;
  });
  elements.sliderRibbonLines.addEventListener('input', (e) => {
    elements.valRibbonLines.textContent = e.target.value;
  });
  elements.sliderMaxLiveDelay.addEventListener('input', (e) => {
    elements.valMaxLiveDelay.textContent = `${e.target.value}s`;
  });
  elements.sliderCatchupInterval.addEventListener('input', (e) => {
    elements.valCatchupInterval.textContent = `${e.target.value}s`;
  });
  elements.sliderVadPad.addEventListener('input', (e) => {
    elements.valVadPad.textContent = `${e.target.value}ms`;
  });
  elements.sliderVadThreshold.addEventListener('input', (e) => {
    elements.valVadThreshold.textContent = `${e.target.value}`;
  });

  // Style tab: live controls, debounced save, presets
  for (const control of [elements.selectSubFont, elements.inputSubFontSize, elements.selectSubWeight,
    elements.selectSubAnimation, elements.inputSubAnimDuration, elements.inputSubLifetime, elements.inputSubLetterSpacing,
    elements.inputSubWordSpacing, elements.inputSubColor,
    elements.inputSubBg, elements.inputSubBgOpacity]) {
    control.addEventListener('input', () => {
      syncLetterSpacingReadout();
      syncWordSpacingReadout();
      renderSubtitlePreview();
      scheduleSubtitleStyleSave();
    });
  }
  elements.inputSubBgCustom.addEventListener('change', () => {
    syncBackgroundControls();
    renderSubtitlePreview();
    scheduleSubtitleStyleSave(0);
  });
  elements.btnSubPresetSave.addEventListener('click', async () => {
    const name = elements.inputSubPresetName.value.trim();
    const check = savePreset(currentConfig.subtitle_presets || [], name, currentSubtitleTheme(), currentConfig.subtitle_style);
    if (check.error) {
      showSubStyleStatus(t(`sub_preset_err_${check.error}`), true);
      return;
    }
    const ok = await showConfirmModal({
      title: t('sub_save_title'),
      message: t('sub_save_message').replace('{name}', name),
      confirmText: t('sub_save_confirm'),
    });
    if (ok) applyPresetSave(name);
  });
  elements.btnSubPresetDelete.addEventListener('click', async () => {
    const name = presetNameFromOption(elements.selectSubtitleStyle.value);
    if (!name) return;
    const ok = await showConfirmModal({
      title: t('sub_delete_title'),
      message: t('sub_delete_message').replace('{name}', name),
      confirmText: t('sub_delete_confirm'),
      isDanger: true,
    });
    if (!ok) return;
    currentConfig = { ...currentConfig, subtitle_presets: deletePreset(currentConfig.subtitle_presets || [], name) };
    renderStyleSelector(currentConfig);
    syncUnsavedStyleChip();
    showSubStyleStatus(t('sub_preset_deleted'));
    scheduleSubtitleStyleSave(0);
  });
  elements.btnSubStyleReset.addEventListener('click', resetSubtitleStyle);

  elements.btnSubPreviewPlay.addEventListener('click', playSubtitlePreview);

  // Style selector: a built-in style or a saved one, saved at once through the style path so the next
  // subtitle and the test subtitle use it.
  elements.selectSubtitleStyle.addEventListener('change', () => {
    if (applyStyleChoice()) scheduleSubtitleStyleSave(0);
  });

  // ASR language change
  elements.selectAsrLang.addEventListener('change', (e) => {
    syncAsrLanguageUi(e.target.value);
  });

  // Prompt suggestion button (loads suggestion based on active spoken language)
  if (elements.btnSuggestPromptEs) {
    elements.btnSuggestPromptEs.addEventListener('click', () => {
      const profileId = elements.selectProfile.value;
      const data = getProfilePresetData(profileId, currentAsrLang);
      if (data.samplePrompt) {
        drafts.prompts[currentAsrLang] = data.samplePrompt;
        elements.promptEs.value = data.samplePrompt;
        showToast(getLanguage() === 'es' ? `Sugerencia cargada (${currentAsrLang.toUpperCase()})` : `Suggestion loaded (${currentAsrLang.toUpperCase()})`);
      }
    });
  }

  bindProfileSelection(elements.selectProfile, {
    getDraft: () => collectFormData(),
    setDraft: (draft) => {
      currentConfig = draft;
      populateForm(currentConfig, false);
    },
    getPresets: () => profilePresets,
    onChange: updateProfileDescription,
  });

  elements.btnOpenSessionFolder.addEventListener('click', async () => {
    await withProcessing(elements.btnOpenSessionFolder, async () => {
      try {
        const path = await invoke('open_session_folder');
        showToast(`${t('toast_session_opened')}${path}`);
      } catch (err) {
        showToast(`${t('toast_session_open_failed')}${err}`, true);
      }
    });
  });

  // Test Subtitle Button
  if (elements.btnTestSubtitle) {
    elements.btnTestSubtitle.addEventListener('click', async () => {
      await withProcessing(elements.btnTestSubtitle, async () => {
        // The backend emits subtitle_cue, which is the only path that appends to the feed.
        const testText = t('test_subtitle_text');
        const style = baseStyle();
        try {
          await invoke('send_test_subtitle', { text: testText, style });
        } catch (err) {
          addSubtitleToFeed(testText, { text: testText, style, timestamp_ms: Date.now() });
        }
      });
    });
  }

  // Copy OBS URL
  elements.btnCopyObsUrl.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(elements.inputObsUrl.value);
      elements.btnCopyObsUrl.textContent = t('btn_copied');
      setTimeout(() => {
        elements.btnCopyObsUrl.textContent = t('btn_copy_url');
      }, 2000);
      showToast(t('toast_obs_copied'));
    } catch (err) {
      showToast(t('toast_obs_copy_failed'), true);
    }
  });

  // Toggle Service Button
  elements.btnToggleService.addEventListener('click', async () => {
    if (isServiceActionPending || currentStatus.monitor_mode) {
      return;
    }
    const targetAction = currentStatus.is_running ? 'stopping' : 'starting';
    isServiceActionPending = true;
    serviceActionPendingMode = targetAction;
    setServiceButtonLoading(targetAction);

    try {
      if (targetAction === 'stopping') {
        appendLogEntry('info', 'Deteniendo servicio de transcripción...', 'SERVICE');
        const newStatus = await invoke('stop_service');
        currentStatus = newStatus;
        appendLogEntry('info', 'Servicio detenido.', 'SERVICE');
      } else {
        appendLogEntry('info', 'Iniciando servicio de transcripción...', 'SERVICE');
        const newStatus = await invoke('start_service');
        currentStatus = newStatus;
        appendLogEntry('info', 'Servicio iniciado correctamente.', 'SERVICE');
      }
    } catch (err) {
      appendLogEntry('error', `Error en servicio: ${err}`, 'ERROR');
      showToast(`${t('status_error')}: ${err}`, true);
    } finally {
      isServiceActionPending = false;
      serviceActionPendingMode = null;
      try {
        const finalStatus = await invoke('get_service_status');
        updateStatusUI(finalStatus);
      } catch (_) {
        updateStatusUI(currentStatus);
      }
    }
  });

  elements.btnRuntimeRepair.addEventListener('click', async () => {
    try {
      appendLogEntry('info', 'Iniciando verificación/reparación de entorno ASR...', 'SETUP');
      setRuntimeSetupProgress({ phase: 'starting', text: t('runtime_setup_starting'), percent: null });
      await invoke('prepare_asr_runtime');
      setRuntimeSetupProgress({ phase: 'ready', text: t('runtime_setup_ready'), percent: 100 });
      appendLogEntry('info', 'Entorno ASR preparado con éxito.', 'SETUP');
    } catch (error) {
      appendLogEntry('error', `Error al preparar entorno ASR: ${error}`, 'SETUP');
      setRuntimeSetupProgress({ phase: 'error', text: String(error), percent: null });
      showToast(`${t('status_error')}: ${error}`, true);
    }
  });

  elements.btnRuntimeCancel.addEventListener('click', async () => {
    try {
      await invoke('cancel_runtime_setup');
    } catch (error) {
      showToast(`${t('status_error')}: ${error}`, true);
    }
  });

  // Save Config Button
  saveGeneralConfig = () => queueConfigSave(async () => {
    await withProcessing(elements.btnSaveConfig, async () => {
      try {
        const updatedConfig = collectFormData();
        let backendConfig = null;
        const result = await persistAndApplyRuntimeConfig({
          before: savedConfig,
          after: updatedConfig,
          isRunning: currentStatus.is_running,
          monitorMode: Boolean(currentStatus.monitor_mode),
          confirmRestart: () => showConfirmModal({
            title: t('modal_title_confirm'),
            message: t('confirm_restart'),
            icon: 'refresh',
          }),
          save: async (config) => {
            backendConfig = await invoke('save_config', { config, base: savedConfig });
          },
          restart: async () => {
            isServiceActionPending = true;
            serviceActionPendingMode = 'starting';
            setServiceButtonLoading('starting');
            try {
              return await invoke('restart_service');
            } finally {
              isServiceActionPending = false;
              serviceActionPendingMode = null;
            }
          },
        });
        if (!result.saved) {
          return;
        }
        adoptSavedConfig(updatedConfig, backendConfig);
        if (result.restarted) {
          updateStatusUI(result.status);
          showToast(t('toast_config_saved_restarted'));
        } else if (result.restartError) {
          updateStatusUI(await invoke('get_service_status'));
          showToast(`${t('toast_config_saved_restart_err')}${result.restartError}`, true);
        } else if (result.deferred) {
          showToast(t('toast_config_saved_deferred'));
        } else {
          showToast(t('toast_config_saved'));
        }
        loadObsOverlayInfo();
        refreshGpuStatus();
        updateStatusUI(currentStatus);
      } catch (err) {
        showToast(`${t('status_error')}: ${err}`, true);
      }
    });
  });
  elements.btnSaveConfig.addEventListener('click', () => saveGeneralConfig());

  // Footer Reset: only the active tab, after a confirmation (the whole-config reset_config_defaults is no longer called from the UI)
  elements.btnResetConfig.addEventListener('click', () => resetActiveTab());

  // Export Diagnostics Button
  elements.btnExportDiag.addEventListener('click', async () => {
    await withProcessing(elements.btnExportDiag, async () => {
      try {
        const result = await invoke('export_diagnostics', {});
        showToast(`${t('toast_diag_exported')}${result.path}`);
      } catch (err) {
        showToast(`${t('toast_diag_failed')}${err}`, true);
      }
    });
  });

  // Initialize Custom Titlebar Window Controls
  setupWindowControls();

  // Load initial data
  try {
    const config = await invoke('get_config');
    populateForm(config);
    renderStyleTab(config, { force: true });
    await loadProfilePresets();
    await loadAudioDevices();
    await loadObsOverlayInfo();
    setupAllowedConnectionsHandlers();
    await refreshAllowedConnections();

    const status = await invoke('get_service_status');
    updateStatusUI(status);
  } catch (err) {
    console.error('Initialization error:', err);
  }

  // Wire up Event Logger Controls
  if (elements.btnToggleLogger) {
    elements.btnToggleLogger.addEventListener('click', toggleLoggerCollapse);
  }
  if (elements.btnClearLogs) {
    elements.btnClearLogs.addEventListener('click', clearLogs);
  }
  if (elements.btnCopyLogs) {
    elements.btnCopyLogs.addEventListener('click', copyLogs);
  }

  // Wire up Subtitles Stream Feed Controls
  if (elements.btnClearFeed) {
    elements.btnClearFeed.addEventListener('click', clearSubtitleFeed);
  }

  if (elements.feedActionMenu) {
    elements.feedActionMenu.addEventListener('click', (e) => {
      const btn = e.target.closest('.feed-menu-btn');
      if (btn) {
        const action = btn.dataset.action;
        handleFeedAction(action);
      }
    });
  }

  // Dismiss feed action menu on click outside, escape key, or container scroll
  document.addEventListener('click', (e) => {
    if (!elements.feedActionMenu || elements.feedActionMenu.hidden) return;
    if (!elements.feedActionMenu.contains(e.target) && !e.target.closest('.btn-feed-item-menu')) {
      closeFeedActionMenu();
    }
  });

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && elements.feedActionMenu && !elements.feedActionMenu.hidden) {
      closeFeedActionMenu();
    }
  });

  if (elements.feedActionMenu) {
    elements.feedActionMenu.addEventListener('keydown', (e) => {
      const menuButtons = Array.from(elements.feedActionMenu.querySelectorAll('.feed-menu-btn'));
      if (menuButtons.length === 0) return;
      const currentIndex = menuButtons.indexOf(document.activeElement);
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        const nextIndex = (currentIndex + 1) % menuButtons.length;
        menuButtons[nextIndex]?.focus();
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        const prevIndex = (currentIndex - 1 + menuButtons.length) % menuButtons.length;
        menuButtons[prevIndex]?.focus();
      } else if (e.key === 'Home') {
        e.preventDefault();
        menuButtons[0]?.focus();
      } else if (e.key === 'End') {
        e.preventDefault();
        menuButtons[menuButtons.length - 1]?.focus();
      } else if (e.key === 'Tab') {
        closeFeedActionMenu();
      }
    });
  }

  if (elements.subtitlesFeedContainer) {
    elements.subtitlesFeedContainer.addEventListener('scroll', () => {
      if (elements.feedActionMenu && !elements.feedActionMenu.hidden) {
        closeFeedActionMenu();
      }
    });
  }

  // Auto-dismiss floating menu when window is resized or outer view is scrolled
  window.addEventListener('resize', () => {
    if (elements.feedActionMenu && !elements.feedActionMenu.hidden) {
      closeFeedActionMenu();
    }
  });
  window.addEventListener('scroll', () => {
    if (elements.feedActionMenu && !elements.feedActionMenu.hidden) {
      closeFeedActionMenu();
    }
  }, true);

  // Load Initial / Recent System Logs
  try {
    const recentLogs = await invoke('get_recent_logs', { maxLines: 40 });
    if (elements.loggerConsole) elements.loggerConsole.innerHTML = '';
    appendLogEntry('info', t('log_events_ready') || 'Sistema listo.', 'SYSTEM');
    if (Array.isArray(recentLogs)) {
      for (const line of recentLogs) {
        const clean = line.replace(/\x1b\[[0-9;]*m/g, '').trim();
        if (!clean) continue;
        if (/ERROR/i.test(clean)) {
          appendLogEntry('error', clean, 'BOOT');
        } else if (/WARN/i.test(clean)) {
          appendLogEntry('warn', clean, 'BOOT');
        }
      }
    }
  } catch (_) {
    appendLogEntry('info', t('log_events_ready') || 'Sistema listo.', 'SYSTEM');
  }

  // Subscribe to Tauri events safely
  try {
    await listen('nvidia_runtime_progress', (event) => {
      const p = event && event.payload;
      if (!p) return;
      elements.nvidiaInstallDetail.textContent = nvidiaProgressText(p);
      if (Number.isFinite(p.percent)) elements.nvidiaInstallMeter.value = Math.max(0, Math.min(100, p.percent));
      else elements.nvidiaInstallMeter.removeAttribute('value');
    });
    let lastLoggedSetup = { phase: null, text: null, milestone: null };
    await listen('runtime_setup_progress', (event) => {
      if (event && event.payload) {
        setRuntimeSetupProgress(event.payload);
        const { phase, text } = event.payload;
        const cleanText = (text || '').trim(); if (!cleanText) return;
        const percent = event.payload.percent != null && Number.isFinite(event.payload.percent)
          ? Math.max(0, Math.min(100, event.payload.percent))
          : null;
        const isErr = phase === 'error';

        if (phase === 'downloading') {
          const milestone = percent != null ? Math.floor(percent / 25) * 25 : null;
          const phaseChanged = phase !== lastLoggedSetup.phase;
          const textChanged = cleanText !== lastLoggedSetup.text;
          const milestoneCrossed = milestone != null && milestone !== lastLoggedSetup.milestone;

          if (phaseChanged || textChanged || milestoneCrossed) {
            lastLoggedSetup = { phase, text: cleanText, milestone };
            const pctStr = percent != null ? ` (${Math.round(percent)}%)` : '';
            appendLogEntry(isErr ? 'error' : 'info', `[${phase}] ${cleanText}${pctStr}`, 'SETUP');
          }
        } else {
          if (phase !== lastLoggedSetup.phase || cleanText !== lastLoggedSetup.text) {
            lastLoggedSetup = { phase, text: cleanText, milestone: null };
            const pctStr = percent != null ? ` (${Math.round(percent)}%)` : '';
            appendLogEntry(isErr ? 'error' : 'info', `[${phase}] ${cleanText}${pctStr}`, 'SETUP');
          }
        }
      }
    });

    let lastLoggedStatus = { is_running: null, asr_state: null, audio_state: null };
    await listen('service_status_changed', (event) => {
      if (event && event.payload) {
        updateStatusUI(event.payload);
        const { is_running, asr_state, audio_state } = event.payload;
        if (!is_running) showFeedPartial('');
        if (
          is_running !== lastLoggedStatus.is_running ||
          asr_state !== lastLoggedStatus.asr_state ||
          audio_state !== lastLoggedStatus.audio_state
        ) {
          lastLoggedStatus = { is_running, asr_state, audio_state };
          const runningLabel = is_running ? 'En ejecución' : 'Detenido';
          let logMsg = `Estado: ${runningLabel} | ASR: ${asr_state}`;
          if (is_running && (audio_state === 'standby' || audio_state === 'reconnecting')) {
            logMsg += ` | Audio: ${audio_state}`;
          }
          appendLogEntry('info', logMsg, 'STATUS');
        }
      }
    });

    await listen('config_changed', (event) => {
      if (event && event.payload) {
        handleExternalConfigChange(event.payload);
      }
    });

    await listen('update-available', (event) => {
      if (event && event.payload) {
        availableUpdate = event.payload;
        renderReleaseNotes(availableUpdate);
        if (elements.updaterStatus && !elements.updaterStatus.textContent) {
          elements.updaterStatus.textContent = t('status_update_available').replace('{version}', availableUpdate.version);
          elements.updaterStatus.className = 'updater-status-text status-available';
        }
        syncUpdaterUI();
      }
    });

    await listen('subtitle_cue', (event) => {
      if (event && event.payload) {
        showFeedPartial('');
        addSubtitleToFeed(event.payload.text, event.payload);
      }
    });
    await listen('subtitle_partial', (event) => {
      if (event && event.payload) showFeedPartial(event.payload.text);
    });
  } catch (err) {
    console.warn('Could not register Tauri event listeners:', err);
  }

  // Wire up In-App Updater controls
  if (elements.btnCheckNow) {
    elements.btnCheckNow.addEventListener('click', checkForUpdatesManual);
  }
  if (elements.checkUpdatesEnabled) {
    elements.checkUpdatesEnabled.addEventListener('change', async () => {
      await queueConfigSave(async () => {
        const newConfig = collectFormData();
        try {
          const saved = await invoke('save_config', { config: newConfig, base: savedConfig });
          adoptSavedConfig(newConfig, saved);
        } catch (err) {
          console.error('Failed to save update toggle state:', err);
        }
      });
    });
  }
  if (elements.btnInstallNow) {
    elements.btnInstallNow.addEventListener('click', installUpdateNow);
  }
  if (elements.btnInstallOnClose) {
    elements.btnInstallOnClose.addEventListener('click', toggleInstallOnClose);
  }
  if (elements.badgeHeaderUpdate) {
    elements.badgeHeaderUpdate.addEventListener('click', () => {
      const advTabBtn = document.getElementById('tab-advanced-tab');
      if (advTabBtn) {
        advTabBtn.click();
        if (elements.groupUpdates) {
          elements.groupUpdates.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }
      }
    });
  }

  try {
    const backup = await invoke('take_startup_warning');
    if (backup) showToast(t('toast_config_corrupt_backup').replace('{name}', backup), true);
  } catch (err) {
    console.warn('Could not query startup warning:', err);
  }

  // Query initial updater status
  try {
    const updaterStatus = await invoke('get_updater_status');
    if (updaterStatus) {
      installOnCloseArmed = Boolean(updaterStatus.install_on_close);
      if (elements.btnInstallOnClose) {
        elements.btnInstallOnClose.setAttribute('aria-pressed', String(installOnCloseArmed));
        elements.btnInstallOnClose.classList.toggle('active', installOnCloseArmed);
      }
      if (updaterStatus.available_update) {
        availableUpdate = updaterStatus.available_update;
        renderReleaseNotes(availableUpdate);
        if (elements.updaterStatus && !elements.updaterStatus.textContent) {
          elements.updaterStatus.textContent = t('status_update_available').replace('{version}', availableUpdate.version);
          elements.updaterStatus.className = 'updater-status-text status-available';
        }
      }
      syncUpdaterUI();
    }
  } catch (err) {
    console.warn('Could not query updater status on init:', err);
  }

  // Heartbeat status poll every 1000ms to guarantee fresh UI status synchronization
  setInterval(async () => {
    try {
      const status = await invoke('get_service_status');
      updateStatusUI(status);
    } catch (_) {}
  }, 1000);
}

function setRuntimeSetupProgress(progress) {
  if (!elements.runtimeSetupProgress || !progress) return;
  const phase = String(progress.phase || 'preparing');
  const text = phase === 'starting_worker' ? t('runtime_setup_worker_starting') : String(progress.text || phase);
  const percent = Number.isFinite(progress.percent) ? Math.max(0, Math.min(100, progress.percent)) : null;
  elements.runtimeSetupProgress.hidden = phase === 'ready';
  elements.runtimeSetupText.textContent = text;
  const isPreparing = ['starting', 'sync', 'publishing', 'starting_worker', 'downloading'].includes(phase);
  const startupFailed = phase === 'error' && !currentStatus.is_running;
  elements.btnRuntimeCancel.hidden = !isPreparing && !startupFailed;
  elements.btnRuntimeRepair.disabled = isPreparing;
  if (percent === null) {
    elements.runtimeSetupMeter.removeAttribute('value');
    elements.runtimeSetupMeter.hidden = false;
    elements.runtimeSetupProgress.removeAttribute('aria-valuenow');
    elements.runtimeSetupProgress.setAttribute('aria-valuetext', text);
  } else {
    elements.runtimeSetupMeter.value = percent;
    elements.runtimeSetupMeter.hidden = false;
    elements.runtimeSetupProgress.setAttribute('aria-valuenow', String(percent));
    elements.runtimeSetupProgress.setAttribute('aria-valuetext', `${Math.round(percent)}%`);
  }
}

// NVIDIA runtime opt-in and GPU/engine status
const NVIDIA_DISMISS_KEY = 'liveaudio.nvidiaOfferDismissed';
let gpuStatus = null;
let nvidiaCardState = 'idle'; // idle | installing | success | cancelled | error
let nvidiaCancelRequested = false;
let nvidiaErrorMessage = '';
const NVIDIA_RESULT_TITLE = { success: 'nvidia_card_success_title', cancelled: 'nvidia_install_cancelled', error: 'nvidia_card_error_title' };

function nvidiaOfferDismissed() {
  try { return localStorage.getItem(NVIDIA_DISMISS_KEY) === '1'; } catch (_) { return false; }
}

function backendLabel(kind) {
  return t(kind === 'ct2_cuda' ? 'opt_asr_backend_ct2' : 'opt_asr_backend_ort');
}

function renderGpuStatus() {
  renderEngineSummary();
  renderNvidiaCard();
  renderEngineDetails();
}

function renderEngineSummary() {
  const live = currentStatus;
  const running = Boolean(live?.is_running);
  const kind = engineKind(gpuStatus, live);
  const reason = running && live.backend_reason ? live.backend_reason : gpuStatus?.reason;
  elements.enginePill.className = running ? 'pill state-listening' : 'pill state-stopped';
  elements.engineStatusText.textContent = kind ? `${t(running ? 'engine_using' : 'engine_will_use')}${t(`engine_name_${kind}`)}` : '';
  elements.engineStatusReason.textContent = reason ? t(gpuReasonKey(reason)) : '';
  elements.btnNvidiaAdvanced.hidden = !nvidiaOfferVisible(gpuStatus, false);
}

function renderEngineDetails() {
  const status = gpuStatus;
  const lines = [];
  if (status) {
    const names = status.adapters.map((a) => `${a.name} (${a.dedicated_vram_mb} MB VRAM)`).join(', ');
    lines.push(status.adapters.length ? `${t('gpu_status_adapters')}${names}` : t('gpu_status_none'));
    const v = status.cuda_driver_version;
    lines.push(v ? `${t('gpu_status_driver')}${Math.floor(v / 1000)}.${Math.floor((v % 1000) / 10)}` : t('gpu_status_driver_none'));
    lines.push(t(status.nvidia_runtime.installed ? 'gpu_status_runtime_installed' : 'gpu_status_runtime_missing'));
    if (!status.nvidia_runtime.installed && status.nvidia_runtime.missing.length) {
      lines.push(`${t('gpu_status_missing_files')}${status.nvidia_runtime.missing.join(', ')}`);
    }
    lines.push(`${t('gpu_status_selected')}${backendLabel(status.selected_backend)}`);
  }
  if (currentStatus?.is_running && currentStatus.active_backend) {
    lines.push(`${t('gpu_status_active')}${backendLabel(currentStatus.active_backend)}`);
  }
  elements.gpuStatusText.textContent = lines.join('\n');
}

function renderNvidiaCard() {
  const state = nvidiaCardView(gpuStatus, currentStatus, nvidiaCardState);
  const offer = nvidiaOfferVisible(gpuStatus, nvidiaOfferDismissed());
  const running = Boolean(currentStatus?.is_running);
  const finished = state === 'success' || state === 'cancelled' || state === 'error';
  elements.cardNvidiaRuntime.hidden = state === 'idle' && !offer;
  elements.nvidiaOffer.hidden = state !== 'idle' || !offer;
  elements.nvidiaGpuName.textContent = gpuStatus?.adapters?.find((a) => a.vendor === 'nvidia')?.name || '';
  elements.nvidiaInstallProgress.hidden = state !== 'installing';
  elements.nvidiaResult.hidden = !finished;
  elements.nvidiaResultTitle.textContent = finished ? t(NVIDIA_RESULT_TITLE[state]) : '';
  elements.nvidiaResultText.textContent = nvidiaResultText(state, running);
  elements.nvidiaErrorDetails.hidden = state !== 'error';
  elements.nvidiaErrorText.textContent = nvidiaErrorMessage;
  elements.btnNvidiaRestart.hidden = !(state === 'success' && running);
  elements.btnNvidiaRetry.hidden = state !== 'error' && state !== 'cancelled';
}

function nvidiaResultText(state, running) {
  if (state === 'success') return t(running ? 'nvidia_card_success_restart' : 'nvidia_card_success_next');
  if (state !== 'error') return '';
  return t(/HTTP status 404/.test(nvidiaErrorMessage) ? 'nvidia_card_unavailable' : 'nvidia_card_error_hint');
}

function nvidiaProgressText(p) {
  if (p.phase !== 'downloading' || !p.total_bytes) return t('nvidia_card_installing');
  const mb = (bytes) => Math.round((bytes || 0) / 1048576);
  return t('nvidia_progress_detail')
    .replace('{percent}', Math.round(p.percent || 0))
    .replace('{done}', mb(p.downloaded_bytes))
    .replace('{total}', mb(p.total_bytes));
}

async function refreshGpuStatus() {
  try {
    gpuStatus = await invoke('get_gpu_backend_status');
  } catch (_) {
    gpuStatus = null;
  }
  renderGpuStatus();
}

async function startNvidiaInstall() {
  if (nvidiaCardState === 'installing' || !gpuStatus) return;
  const accepted = await showConfirmModal({
    title: t('nvidia_confirm_title'),
    message: t('nvidia_confirm_message'),
    detail: gpuStatus.license_urls.length ? t('nvidia_licenses_label') : '',
    links: gpuStatus.license_urls,
    confirmText: t('nvidia_confirm_accept'),
    icon: 'download',
  });
  if (!accepted) return;
  nvidiaCardState = 'installing';
  nvidiaCancelRequested = false;
  elements.nvidiaInstallMeter.value = 0;
  elements.nvidiaInstallDetail.textContent = t('nvidia_card_installing');
  renderGpuStatus();
  try {
    await invoke('install_nvidia_runtime');
    nvidiaCardState = 'success';
    await useNvidiaOnNextStart();
  } catch (err) {
    nvidiaErrorMessage = String(err);
    nvidiaCardState = nvidiaCancelRequested ? 'cancelled' : 'error';
  } finally {
    await refreshGpuStatus();
  }
}

// Saves device cuda and asr_backend auto (unless the user chose ort) so the next start really uses the runtime.
function useNvidiaOnNextStart() {
  return queueConfigSave(async () => {
    const next = savedConfig && nvidiaInstallConfig(savedConfig);
    if (!next) return;
    try {
      adoptSavedConfig(next, await invoke('save_config', { config: next, base: savedConfig }));
    } catch (err) {
      showToast(`${t('status_error')}: ${err}`, true);
    }
  });
}

async function restartServiceAfterNvidia() {
  try {
    const status = await invoke('restart_service');
    nvidiaCardState = 'idle';
    updateStatusUI(status);
  } catch (err) {
    showToast(`${t('status_error')}: ${err}`, true);
  }
}

// Start application when DOM is ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', initApp);
} else {
  initApp();
}
