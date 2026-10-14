// SPDX-License-Identifier: MIT

/**
 * LiveAudio Tauri Desktop Frontend Application Logic
 */

import { audioStateKey, bindProfileSelection, bindTabNavigation, hardwareStatusLabel, monitorBannerText, monitorFeedAction, monitorFeedText, persistAndApplyRuntimeConfig, sessionFailureLabel } from './runtime-settings.js';
import { initLanguage, setLanguage, getLanguage, t, applyTranslations, getProfilePresetData, PROFILE_I18N } from './i18n.js';

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
        subtitle_display_mode: 'adaptive',
        subtitle_ribbon_max_lines: 3,
        subtitle_backlog_policy: 'auto',
        subtitle_max_live_delay_sec: 10.0,
        subtitle_catchup_interval_sec: 1.5,
        silence_timeout: 0.8,
        max_chunk_duration: 5.0,
        transcription_purpose: 'subtitles',
        asr_decode_timeout_sec: 15,
        audio_device: null,
        selected_profile_id: 'balanced',
        profile_mode: 'preset',
        ws_port: 8765,
        obs_enabled: true,
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

    case 'save_config':
      return Promise.resolve(true);

    case 'prepare_asr_runtime':
      return Promise.resolve();

    case 'cancel_runtime_setup':
      return Promise.resolve(true);

    case 'get_profile_presets':
      return Promise.resolve([
        { id: 'fast', label: 'Rápido', description: 'Menos demora y frases cortas; baja un poco la precisión.', device: 'cpu', model_size: 'base (Rápido)', silence_timeout: 0.4, subtitle_backlog_policy: 'live_only', subtitle_max_live_delay_sec: 5, subtitle_catchup_interval_sec: 0.8 },
        { id: 'balanced', label: 'Balanceado', description: 'Recomendado para la mayoría de sesiones.', device: 'cuda', model_size: 'small (Balance CPU)', silence_timeout: 0.8, subtitle_backlog_policy: 'auto', subtitle_max_live_delay_sec: 10, subtitle_catchup_interval_sec: 1.5 },
        { id: 'quality', label: 'Calidad', description: 'Más precisión; puede usar más VRAM y tardar más.', device: 'cuda', model_size: 'turbo (Máxima precisión GPU)', silence_timeout: 1, subtitle_backlog_policy: 'auto', subtitle_max_live_delay_sec: 15, subtitle_catchup_interval_sec: 2 },
        { id: 'stable_streaming', label: 'Streaming estable', description: 'Reduce carga de GPU para jugar o transmitir en PC ocupada.', device: 'cpu', model_size: 'small (Balance CPU)', silence_timeout: 0.6, subtitle_backlog_policy: 'live_only', subtitle_max_live_delay_sec: 6, subtitle_catchup_interval_sec: 1 },
      ]);

    case 'open_session_folder':
      return Promise.resolve('E:\\LiveAudio\\sessions\\session-demo');

    case 'reset_config_defaults':
      return mockInvoke('get_config', {});

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
        url: 'file:///E:/LiveAudio/liveaudio/assets/subtitulos_obs.html?port=8765',
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
        '🚀 LiveAudio Desktop Logging Initialized',
        '📁 Log file: C:\\Users\\...\\logs\\liveaudio-desktop.log',
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
  selectDisplayMode: document.getElementById('select-display-mode'),
  sliderRibbonLines: document.getElementById('slider-ribbon-lines'),
  valRibbonLines: document.getElementById('val-ribbon-lines'),
  selectBacklogPolicy: document.getElementById('select-backlog-policy'),
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
  checkAcousticEventsEnabled: document.getElementById('check-acoustic-events-enabled'),
  sessionPath: document.getElementById('session-path'),
  sessionError: document.getElementById('session-error'),
  monitorBanner: document.getElementById('monitor-banner'),
  btnOpenSessionFolder: document.getElementById('btn-open-session-folder'),
  cardSubtitlesFeed: document.getElementById('card-subtitles-feed'),
  subtitlesFeedContainer: document.getElementById('subtitles-feed-container'),
  feedEmptyState: document.getElementById('feed-empty-state'),
  feedCountBadge: document.getElementById('feed-count-badge'),
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
function showToast(message, isError = false) {
  const toast = document.createElement('div');
  toast.className = `toast ${isError ? 'toast-error' : ''}`;
  toast.textContent = message;
  elements.toastContainer.appendChild(toast);
  setTimeout(() => {
    toast.remove();
  }, 3500);
}

// Master button transition state tracking to prevent race conditions and enforce idempotency
let isServiceActionPending = false;
let serviceActionPendingMode = null; // 'starting' | 'stopping'

// Update Status Pills and Control Bar
function updateStatusUI(status) {
  currentStatus = status;

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
  const cssState = state === 'monitor' ? 'listening' : state.startsWith('error') ? 'failed' : (state.startsWith('Descargando') || state === 'downloading') ? 'starting' : state;
  elements.pillStatus.className = `pill state-${cssState}`;
  const stateLabels = {
    stopped: t('status_stopped'),
    starting: t('status_starting'),
    downloading: t('status_downloading'),
    ready: t('status_ready'),
    listening: t('status_listening'),
    transcribing: t('status_transcribing'),
    degraded: t('status_degraded'),
    failed: t('status_failed'),
    monitor: t('status_monitor'),
  };
  elements.pillStatusText.textContent = stateLabels[state] || state;

  // VAD Pill
  if (status.vad_onset) {
    elements.pillVad.classList.add('active');
    elements.pillVadText.textContent = t('vad_speech');
  } else {
    elements.pillVad.classList.remove('active');
    elements.pillVadText.textContent = t('vad_silence');
  }

  // WS Pill
  elements.pillWsText.textContent = `WS: ${status.ws_port} (${status.ws_clients} ${t('ws_clients_suffix')})`;

  // Hardware Pill
  const audioKey = audioStateKey(status);
  const hwLabel = audioKey ? `${hardwareStatusLabel(status)} | ${t(audioKey)}` : hardwareStatusLabel(status);
  elements.pillHw.textContent = hwLabel;
  elements.pillHw.title = hwLabel;
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
  syncMonitorFeed(status);
}

// Read-only subtitle feed from a service another process owns (same protocol as the OBS overlay).
let monitorFeed = null; // { socket, port }

function syncMonitorFeed(status) {
  const action = monitorFeedAction(status, monitorFeed ? monitorFeed.port : null);
  if (action === 'keep') return;
  if (monitorFeed) monitorFeed.socket.close();
  monitorFeed = null;
  if (action !== 'open') return;
  // A dropped connection clears the feed; the next status poll (1 s) reconnects it.
  const feed = { socket: new WebSocket(`ws://127.0.0.1:${status.ws_port}`), port: status.ws_port };
  feed.socket.onmessage = (event) => addSubtitleToFeed(monitorFeedText(event.data));
  feed.socket.onclose = () => {
    if (monitorFeed === feed) monitorFeed = null;
  };
  monitorFeed = feed;
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

function addSubtitleToFeed(text, cue = null) {
  if (!elements.subtitlesFeedContainer) return;
  const cleanText = (text || '').trim();
  if (!cleanText) return;

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
      await invoke('save_config', { config: newConfig });
      currentConfig = newConfig;
      savedConfig = structuredClone(newConfig);
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
      await invoke('save_config', { config: newConfig });
      currentConfig = newConfig;
      savedConfig = structuredClone(newConfig);
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

async function copyTextToClipboard(text) {
  const trimmed = text.trim();
  if (!trimmed) return;
  try {
    if (navigator.clipboard && navigator.clipboard.writeText) {
      await navigator.clipboard.writeText(trimmed);
    } else {
      copyTextFallback(trimmed);
    }
    showToast(t('toast_text_copied'));
  } catch (err) {
    try {
      copyTextFallback(trimmed);
      showToast(t('toast_text_copied'));
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

// Accessible Modal Dialog System (replaces window.alert and window.confirm)
function showConfirmModal({
  title = t('modal_title_confirm'),
  message = '',
  detail = '',
  confirmText = t('modal_confirm'),
  cancelText = t('modal_cancel'),
  isDanger = false,
  icon = '⚠️',
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
    iconEl.textContent = icon;
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

  // Model Tab
  for (let opt of elements.selectModelSize.options) {
    if (opt.value.startsWith(config.model_size.split(' ')[0])) {
      opt.selected = true;
      break;
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
  elements.selectSubtitleStyle.value = config.subtitle_style || 'default';
  elements.selectDisplayMode.value = config.subtitle_display_mode || 'adaptive';

  elements.sliderRibbonLines.value = config.subtitle_ribbon_max_lines || 3;
  elements.valRibbonLines.textContent = config.subtitle_ribbon_max_lines || 3;

  elements.selectBacklogPolicy.value = config.subtitle_backlog_policy || 'auto';

  elements.sliderMaxLiveDelay.value = config.subtitle_max_live_delay_sec || 10.0;
  elements.valMaxLiveDelay.textContent = `${config.subtitle_max_live_delay_sec || 10.0}s`;

  // Advanced Tab
  elements.inputWsPort.value = config.ws_port || 8765;
  elements.sliderVadPad.value = config.vad_speech_pad_ms || 200;
  elements.valVadPad.textContent = `${config.vad_speech_pad_ms || 200}ms`;

  elements.sliderVadThreshold.value = config.vad_threshold || 0.5;
  elements.valVadThreshold.textContent = `${config.vad_threshold || 0.5}`;

  elements.checkContinuous.checked = Boolean(config.continuous_session);
  elements.checkSaveTranscript.checked = Boolean(config.save_transcript_enabled);
  elements.checkSaveVtt.checked = Boolean(config.save_vtt_enabled);
  elements.checkRnnoiseEnabled.checked = Boolean(config.rnnoise_enabled);
  elements.checkAcousticEventsEnabled.checked = Boolean(config.acoustic_events_enabled);
  elements.selectProfile.value = config.profile_mode === 'custom' ? 'custom' : (config.selected_profile_id || 'balanced');
  elements.sliderCatchupInterval.value = config.subtitle_catchup_interval_sec || 1.5;
  elements.valCatchupInterval.textContent = `${config.subtitle_catchup_interval_sec || 1.5}s`;

  updateProfileDescription();
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

  cfg.subtitle_style = elements.selectSubtitleStyle.value;
  cfg.subtitle_display_mode = elements.selectDisplayMode.value;
  cfg.subtitle_ribbon_max_lines = parseInt(elements.sliderRibbonLines.value, 10);
  cfg.subtitle_backlog_policy = elements.selectBacklogPolicy.value;
  cfg.subtitle_max_live_delay_sec = parseFloat(elements.sliderMaxLiveDelay.value);
  cfg.subtitle_catchup_interval_sec = parseFloat(elements.sliderCatchupInterval.value);

  cfg.ws_port = parseInt(elements.inputWsPort.value, 10) || 8765;
  cfg.vad_speech_pad_ms = parseInt(elements.sliderVadPad.value, 10);
  cfg.vad_threshold = parseFloat(elements.sliderVadThreshold.value);
  cfg.continuous_session = elements.checkContinuous.checked;
  cfg.save_transcript_enabled = elements.checkSaveTranscript.checked;
  cfg.save_vtt_enabled = elements.checkSaveVtt.checked;
  cfg.rnnoise_enabled = elements.checkRnnoiseEnabled.checked;
  cfg.acoustic_events_enabled = elements.checkAcousticEventsEnabled.checked;
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
  } catch (err) {
    console.error('Error loading OBS overlay info:', err);
  }
}

// Initialize Application
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

  bindTabNavigation(document.querySelector('.tab-nav'));

  elements.authorProfileLink.addEventListener('click', async (event) => {
    if (!isTauri) return;
    event.preventDefault();
    try {
      await invoke('open_author_profile');
    } catch (err) {
      showToast(`${t('toast_author_profile_open_failed')}${err}`, true);
    }
  });

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

  // Subtitle Style change
  elements.selectSubtitleStyle.addEventListener('change', () => {
    // Style will be applied to future subtitles in OBS overlay
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
        const samplePhrases = getLanguage() === 'es' ? [
          '¡Hola mundo! Transcripción de voz en tiempo real con LiveAudio.',
          'Excelente fidelidad acústica en español e inglés sin retraso.',
          'Integración nativa con OBS Studio mediante fuente de navegador.',
          'Tecnología Whisper con aceleración por hardware CUDA de baja latencia.',
        ] : [
          'Hello world! Real-time voice transcription powered by LiveAudio.',
          'High acoustic fidelity in Spanish and English with ultra-low latency.',
          'Native OBS Studio integration via local browser source.',
          'Whisper ASR engine with GPU CUDA hardware acceleration.',
        ];
        const phrase = samplePhrases[Math.floor(Math.random() * samplePhrases.length)];
        const style = elements.selectSubtitleStyle.value;
        try {
          const cue = await invoke('send_test_subtitle', { text: phrase, style });
          addSubtitleToFeed(cue.text, cue);
        } catch (err) {
          addSubtitleToFeed(phrase, { text: phrase, style, timestamp_ms: Date.now() });
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
  elements.btnSaveConfig.addEventListener('click', async () => {
    await withProcessing(elements.btnSaveConfig, async () => {
      try {
        const updatedConfig = collectFormData();
        const result = await persistAndApplyRuntimeConfig({
          before: savedConfig,
          after: updatedConfig,
          isRunning: currentStatus.is_running,
          monitorMode: Boolean(currentStatus.monitor_mode),
          confirmRestart: () => showConfirmModal({
            title: t('modal_title_confirm'),
            message: t('confirm_restart'),
            icon: '🔄',
          }),
          save: (config) => invoke('save_config', { config }),
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
        currentConfig = updatedConfig;
        savedConfig = structuredClone(updatedConfig);
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
        updateStatusUI(currentStatus);
      } catch (err) {
        showToast(`${t('status_error')}: ${err}`, true);
      }
    });
  });

  // Reset Defaults Button
  elements.btnResetConfig.addEventListener('click', async () => {
    await withProcessing(elements.btnResetConfig, async () => {
      try {
        const restart = currentStatus.is_running;
        if (restart) {
          const ok = await showConfirmModal({
            title: t('modal_title_confirm'),
            message: t('confirm_reset'),
            icon: '⚠️',
            isDanger: true,
          });
          if (!ok) {
            return;
          }
        }
        const defaults = await invoke('reset_config_defaults');
        populateForm(defaults);
        if (restart) {
          try {
            isServiceActionPending = true;
            serviceActionPendingMode = 'starting';
            setServiceButtonLoading('starting');
            const status = await invoke('restart_service');
            updateStatusUI(status);
          } catch (err) {
            updateStatusUI(await invoke('get_service_status'));
            showToast(`${t('toast_config_saved_restart_err')}${err}`, true);
            return;
          } finally {
            isServiceActionPending = false;
            serviceActionPendingMode = null;
          }
        }
        showToast(restart
          ? t('toast_config_reset_restarted')
          : currentStatus.monitor_mode ? t('toast_config_saved_deferred') : t('toast_config_reset'));
      } catch (err) {
        showToast(`${t('status_error')}: ${err}`, true);
      }
    });
  });

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
    await loadProfilePresets();
    await loadAudioDevices();
    await loadObsOverlayInfo();

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
    const recentLogs = await invoke('get_recent_logs', { max_lines: 40 });
    if (Array.isArray(recentLogs) && recentLogs.length > 0) {
      if (elements.loggerConsole) elements.loggerConsole.innerHTML = '';
      for (const line of recentLogs) {
        const clean = line.replace(/\x1b\[[0-9;]*m/g, '').trim();
        if (!clean) continue;
        let lvl = 'info';
        if (/ERROR/i.test(clean)) lvl = 'error';
        else if (/WARN/i.test(clean)) lvl = 'warn';
        else if (/ASR/i.test(clean)) lvl = 'asr';
        else if (/VAD/i.test(clean)) lvl = 'vad';
        appendLogEntry(lvl, clean, 'INIT');
      }
    } else {
      appendLogEntry('info', 'Panel de eventos listo.', 'SYSTEM');
    }
  } catch (_) {
    appendLogEntry('info', 'Panel de eventos listo.', 'SYSTEM');
  }

  // Subscribe to Tauri events safely
  try {
    await listen('runtime_setup_progress', (event) => {
      if (event && event.payload) {
        setRuntimeSetupProgress(event.payload);
        const isErr = event.payload.phase === 'error';
        const pctStr = event.payload.percent != null ? ` (${Math.round(event.payload.percent)}%)` : '';
        appendLogEntry(isErr ? 'error' : 'info', `[${event.payload.phase}] ${event.payload.text}${pctStr}`, 'SETUP');
      }
    });

    await listen('service_status_changed', (event) => {
      if (event && event.payload) {
        updateStatusUI(event.payload);
        const runningLabel = event.payload.is_running ? 'En ejecución' : 'Detenido';
        appendLogEntry('info', `Estado: ${runningLabel} | ASR: ${event.payload.asr_state}`, 'STATUS');
      }
    });

    await listen('subtitle_cue', (event) => {
      if (event && event.payload) {
        addSubtitleToFeed(event.payload.text, event.payload);
      }
    });
  } catch (err) {
    console.warn('Could not register Tauri event listeners:', err);
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
  const text = String(progress.text || phase);
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

// Start application when DOM is ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', initApp);
} else {
  initApp();
}
