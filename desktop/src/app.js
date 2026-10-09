// SPDX-License-Identifier: MIT

/**
 * LiveAudio Tauri Desktop Frontend Application Logic
 */

import { bindProfileSelection, bindTabNavigation, hardwareStatusLabel, persistAndApplyRuntimeConfig, sessionFailureLabel } from './runtime-settings.js';
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
  btnSaveConfig: document.getElementById('btn-save-config'),
  btnResetConfig: document.getElementById('btn-reset-config'),
  btnExportDiag: document.getElementById('btn-export-diag'),
  audioDeviceSelect: document.getElementById('select-audio-device'),
  selectComputeDevice: document.getElementById('select-compute-device'),
  deviceRadioCpu: document.getElementById('device-cpu'),
  deviceRadioCuda: document.getElementById('device-cuda'),
  sliderCpuThreads: document.getElementById('slider-cpu-threads'),
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
  sessionPath: document.getElementById('session-path'),
  sessionError: document.getElementById('session-error'),
  btnOpenSessionFolder: document.getElementById('btn-open-session-folder'),
  subtitleBox: document.getElementById('subtitle-box'),
  btnTestSubtitle: document.getElementById('btn-test-subtitle'),
  inputObsUrl: document.getElementById('input-obs-url'),
  btnCopyObsUrl: document.getElementById('btn-copy-obs-url'),
  toastContainer: document.getElementById('toast-container'),
  winBtnMinimize: document.getElementById('win-btn-minimize'),
  winBtnMaximize: document.getElementById('win-btn-maximize'),
  winBtnClose: document.getElementById('win-btn-close'),
  winIconRect: document.getElementById('win-icon-rect'),
};

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

  // Master Toggle Button - Real-time state handling & anti-race conditions
  const isTransitioning = isServiceActionPending
    || status.asr_state === 'starting'
    || status.asr_state === 'stopping'
    || (status.asr_state && status.asr_state.startsWith('Descargando'));

  if (isTransitioning) {
    const mode = (serviceActionPendingMode === 'stopping' || status.asr_state === 'stopping')
      ? 'stopping'
      : 'starting';
    setServiceButtonLoading(mode);
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
  const state = status.asr_state || (status.is_running ? 'listening' : 'stopped');
  const cssState = state.startsWith('error') ? 'failed' : state.startsWith('Descargando') ? 'starting' : state;
  elements.pillStatus.className = `pill state-${cssState}`;
  const stateLabels = {
    stopped: t('status_stopped'),
    starting: t('status_starting'),
    ready: t('status_ready'),
    listening: t('status_listening'),
    transcribing: t('status_transcribing'),
    degraded: t('status_degraded'),
    failed: t('status_failed'),
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
  const hwLabel = hardwareStatusLabel(status);
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
}

// Render Subtitle in Preview Box
function renderSubtitle(text, style) {
  elements.subtitleBox.className = `sub-box style-${style}`;

  if (style === 'karaoke' || style === 'rgb' || style === 'typewriter') {
    const words = text.split(/\s+/).filter(Boolean);
    elements.subtitleBox.innerHTML = words
      .map((w, idx) => `<span class="word" style="animation-delay: ${idx * 0.08}s">${escapeHtml(w)}</span>`)
      .join(' ');
  } else {
    elements.subtitleBox.textContent = text;
  }

  if (subtitleClearTimeout) {
    clearTimeout(subtitleClearTimeout);
  }
  subtitleClearTimeout = setTimeout(() => {
    // Fade out or keep static
  }, 6000);
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
    elements.selectComputeDevice.value = config.device || 'cuda';
  } else if (config.device === 'cuda') {
    if (elements.deviceRadioCuda) elements.deviceRadioCuda.checked = true;
  } else {
    if (elements.deviceRadioCpu) elements.deviceRadioCpu.checked = true;
  }

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
  elements.selectProfile.value = config.profile_mode === 'custom' ? 'custom' : (config.selected_profile_id || 'balanced');
  elements.sliderCatchupInterval.value = config.subtitle_catchup_interval_sec || 1.5;
  elements.valCatchupInterval.textContent = `${config.subtitle_catchup_interval_sec || 1.5}s`;

  updateProfileDescription();

  // Update initial preview theme
  renderSubtitle(t('sample_preview_text'), config.subtitle_style);
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
      renderSubtitle(t('sample_preview_text'), elements.selectSubtitleStyle.value);
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

  // Subtitle Style Preview change
  elements.selectSubtitleStyle.addEventListener('change', (e) => {
    const text = elements.subtitleBox.textContent || t('sample_preview_text');
    renderSubtitle(text, e.target.value);
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
        renderSubtitle(cue.text, cue.style);
      } catch (err) {
        renderSubtitle(phrase, style);
      }
    });
  });

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
    if (isServiceActionPending) {
      return;
    }
    const targetAction = currentStatus.is_running ? 'stopping' : 'starting';
    isServiceActionPending = true;
    serviceActionPendingMode = targetAction;
    setServiceButtonLoading(targetAction);

    try {
      if (targetAction === 'stopping') {
        const newStatus = await invoke('stop_service');
        currentStatus = newStatus;
        showToast(t('toast_service_stopped'));
      } else {
        const newStatus = await invoke('start_service');
        currentStatus = newStatus;
        showToast(t('toast_service_started'));
      }
    } catch (err) {
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

  // Save Config Button
  elements.btnSaveConfig.addEventListener('click', async () => {
    await withProcessing(elements.btnSaveConfig, async () => {
      try {
        const updatedConfig = collectFormData();
        const result = await persistAndApplyRuntimeConfig({
          before: savedConfig,
          after: updatedConfig,
          isRunning: currentStatus.is_running,
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
          : t('toast_config_reset'));
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

  // Subscribe to Tauri events safely
  try {
    await listen('service_status_changed', (event) => {
      if (event && event.payload) {
        updateStatusUI(event.payload);
      }
    });

    await listen('subtitle_cue', (event) => {
      if (event && event.payload) {
        renderSubtitle(event.payload.text, event.payload.style || elements.selectSubtitleStyle.value);
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

// Start application when DOM is ready
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', initApp);
} else {
  initApp();
}
