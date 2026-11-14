// Runtime-affecting settings require a confirmed service restart to take effect.
const RUNTIME_SETTINGS = [
  'device',
  'model_size',
  'audio_device',
  'ws_port',
  'asr_language',
  'language',
  'whisper_context_prompt_es',
  'whisper_context_prompt_en',
  'silence_timeout',
  'max_chunk_duration',
  'vad_speech_pad_ms',
  'vad_threshold',
  'subtitle_backlog_policy',
  'subtitle_max_live_delay_sec',
  'subtitle_catchup_interval_sec',
  'output_dir',
  'continuous_session',
  'save_transcript_enabled',
  'save_vtt_enabled',
  'rnnoise_enabled',
  'acoustic_events_enabled',
  'live_partial_subtitles_enabled',
  'asr_backend',
];

export function runtimeSettingsChanged(before, after) {
  return RUNTIME_SETTINGS.some((key) => JSON.stringify(before?.[key]) !== JSON.stringify(after?.[key]));
}

// Fields owned by the Style tab's save path. The general form never merges them.
export const STYLE_CONFIG_KEYS = Object.freeze(['subtitle_style', 'subtitle_theme', 'subtitle_presets']);

export function styleFields(config) {
  return Object.fromEntries(STYLE_CONFIG_KEYS.map((key) => [key, config[key]]));
}

// The general form's fields only: a pending Style tab edit is not an unsaved general edit.
export function generalFields(config) {
  return Object.fromEntries(Object.entries(config).filter(([key]) => !STYLE_CONFIG_KEYS.includes(key)));
}

// After a general save, the form shows the backend's config for fields the user has not touched
// since the save was sent; fields edited while the save was in flight keep the user's value.
// Style fields are skipped: the form keeps its own copy, which the Style tab keeps current.
export function mergeSavedConfigIntoForm(sent, formNow, saved) {
  const merged = { ...formNow };
  for (const key of Object.keys(saved)) {
    if (STYLE_CONFIG_KEYS.includes(key)) continue;
    if (JSON.stringify(formNow[key]) === JSON.stringify(sent[key])) merged[key] = saved[key];
  }
  return merged;
}

// A Style save adopts the style fields the backend returned and nothing else.
export function adoptSavedStyle(config, saved) {
  const next = { ...config };
  for (const key of STYLE_CONFIG_KEYS) next[key] = saved[key];
  return next;
}

// What to do when config.json changed outside this window: a clean form adopts the new values,
// a form with unsaved edits keeps them (the save merges with the file either way).
export function externalConfigUpdate({ hasUnsavedEdits, restartRequired, isRunning }) {
  let messageKey = hasUnsavedEdits ? 'toast_config_changed_pending' : 'toast_config_changed';
  if (restartRequired && isRunning) messageKey = 'toast_config_changed_restart';
  return { refreshForm: !hasUnsavedEdits, messageKey };
}

export function applyProfilePreset(draft, profile) {
  return {
    ...draft,
    selected_profile_id: profile.id,
    profile_mode: 'preset',
    device: profile.device,
    model_size: profile.model_size,
    silence_timeout: profile.silence_timeout,
    subtitle_backlog_policy: profile.subtitle_backlog_policy,
    subtitle_max_live_delay_sec: profile.subtitle_max_live_delay_sec,
    subtitle_catchup_interval_sec: profile.subtitle_catchup_interval_sec,
    ...(typeof profile.rnnoise_enabled === 'boolean' ? { rnnoise_enabled: profile.rnnoise_enabled } : {}),
    ...(typeof profile.acoustic_events_enabled === 'boolean'
      ? { acoustic_events_enabled: profile.acoustic_events_enabled }
      : {}),
  };
}

export function bindProfileSelection(selectElement, { getDraft, setDraft, getPresets, onChange }) {
  selectElement.addEventListener('change', () => {
    const draft = getDraft();
    const profile = getPresets().find((item) => item.id === selectElement.value);
    if (profile && draft) {
      setDraft(applyProfilePreset(draft, profile));
    } else if (draft) {
      setDraft({ ...draft, profile_mode: 'custom', selected_profile_id: 'custom' });
    }
    onChange();
  });
}

export function bindTabNavigation(tablist, { canLeave, onActivate } = {}) {
  const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'));
  if (tabs.length === 0) return;

  const panels = tabs.map((tab) => tablist.ownerDocument.getElementById(tab.getAttribute('aria-controls')));
  const activate = (activeIndex, moveFocus = false) => {
    tabs.forEach((tab, index) => {
      const active = index === activeIndex;
      tab.classList.toggle('active', active);
      tab.setAttribute('aria-selected', String(active));
      tab.tabIndex = active ? 0 : -1;
      if (panels[index]) {
        panels[index].hidden = !active;
        panels[index].classList.toggle('active', active);
      }
    });
    if (moveFocus) tabs[activeIndex].focus();
    onActivate?.(tabs[activeIndex]);
  };

  // `canLeave(fromTab, toTab)` may be async and returns false to stay on the current tab.
  let deciding = false;
  const select = (index, moveFocus = false) => {
    const current = tabs.findIndex((tab) => tab.classList.contains('active'));
    if (!canLeave || index === current) {
      activate(index, moveFocus);
      return undefined;
    }
    if (deciding) return undefined;
    deciding = true;
    return Promise.resolve(canLeave(tabs[current], tabs[index]))
      .then((ok) => { if (ok) activate(index, moveFocus); })
      .finally(() => { deciding = false; });
  };

  const initialIndex = Math.max(0, tabs.findIndex((tab) => tab.classList.contains('active')));
  activate(initialIndex);

  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => select(index));
    tab.addEventListener('keydown', (event) => {
      let nextIndex;
      switch (event.key) {
        case 'ArrowRight':
          nextIndex = (index + 1) % tabs.length;
          break;
        case 'ArrowLeft':
          nextIndex = (index - 1 + tabs.length) % tabs.length;
          break;
        case 'Home':
          nextIndex = 0;
          break;
        case 'End':
          nextIndex = tabs.length - 1;
          break;
        default:
          return;
      }
      event.preventDefault();
      select(nextIndex, true);
    });
  });
}

export function hardwareStatusLabel(status) {
  const runtimeLabel = status.is_running ? 'Activo' : 'Configurado';
  let deviceLabel = (status.device || 'cpu').toUpperCase();

  if (status.is_running && status.effective_device) {
    const eff = String(status.effective_device).toLowerCase();
    if (eff === 'cuda') {
      deviceLabel = 'GPU (CUDA)';
    } else if (eff === 'directml' || eff === 'dml') {
      deviceLabel = 'GPU (DirectML)';
    } else if (eff === 'cpu') {
      deviceLabel = status.fallback_occurred ? 'CPU (Fallback)' : 'CPU';
    } else {
      deviceLabel = status.effective_device.toUpperCase();
    }
  }

  const modelPart = (status.model_size || '').split(' ')[0] || 'base';
  return `${runtimeLabel}: ${modelPart} | ${deviceLabel} | ${status.active_device || 'Default input'}`;
}

/** i18n key for a live audio capture state worth surfacing, or '' while capturing or stopped. */
export function audioStateKey(status) {
  return status.is_running && ['standby', 'reconnecting'].includes(status.audio_state)
    ? `audio_${status.audio_state}`
    : '';
}

/** Read-only feed step for a service owned by another process: 'open', 'close' or 'keep'. */
export function monitorFeedAction(status, feedPort) {
  const wantedPort = status?.monitor_mode ? status.ws_port : null;
  if (wantedPort === feedPort) return 'keep';
  return wantedPort ? 'open' : 'close';
}

/** Subtitle text carried by a service WebSocket frame, or '' for control or malformed frames. */
export function monitorFeedText(raw) {
  try {
    const data = JSON.parse(raw);
    return data && data.type !== 'hello' && typeof data.text === 'string' ? data.text : '';
  } catch (_) {
    return '';
  }
}

export function monitorBannerText(status, translate) {
  if (!status?.monitor_mode) return '';
  const owner = status.foreign_service_pid ? `PID ${status.foreign_service_pid}` : translate('monitor_pid_unknown');
  return translate('monitor_banner').replace('{pid}', owner);
}

export function sessionFailureLabel(status) {
  return status.session_error || '';
}

/**
 * Saves settings, restarting this desktop's service after confirmation when needed. In Monitor
 * Mode another process owns the service: runtime changes are only saved (`deferred`), never applied.
 */
export async function persistAndApplyRuntimeConfig({ before, after, isRunning, monitorMode = false, confirmRestart, save, restart }) {
  const shouldRestart = Boolean(isRunning && runtimeSettingsChanged(before, after));
  if (shouldRestart && !(await confirmRestart())) {
    return { saved: false, restarted: false, status: null };
  }
  await save(after);
  if (!shouldRestart) {
    const deferred = Boolean(monitorMode && runtimeSettingsChanged(before, after));
    return { saved: true, restarted: false, status: null, deferred };
  }
  try {
    return { saved: true, restarted: true, status: await restart(), restartError: null };
  } catch (restartError) {
    return { saved: true, restarted: false, status: null, restartError };
  }
}

export function isServiceTransitioning(isPending, asrState) {
  return Boolean(
    isPending
    || asrState === 'starting'
    || asrState === 'stopping'
    || (asrState && asrState.startsWith('Descargando'))
  );
}

const GPU_REASONS = ['no_nvidia_gpu', 'cuda_driver_missing', 'cuda_driver_too_old', 'nvidia_runtime_not_installed',
  'nvidia_cuda_ready', 'user_selected_ort', 'ct2_preflight_failed', 'ct2_start_failed'];

/** i18n key for a backend selection reason code; unknown codes fall back to a generic message. */
export function gpuReasonKey(code) {
  return GPU_REASONS.includes(code) ? `gpu_reason_${code}` : 'gpu_reason_unknown';
}

/** Minimum CUDA driver version (major * 1000 + minor * 10) for the NVIDIA runtime. */
export const NVIDIA_MIN_DRIVER = 12080;

/**
 * Engine a status points to, as 'cuda', 'directml' or 'cpu': the live engine while the service runs,
 * otherwise the selected one (ONNX uses DirectML when a GPU is present). Null when nothing is known.
 */
export function engineKind(gpu, live) {
  if (live?.is_running && live.active_backend) {
    const device = String(live.effective_device || '').toLowerCase();
    if (live.active_backend === 'ct2_cuda' || device === 'cuda') return 'cuda';
    return device === 'directml' || device === 'dml' ? 'directml' : 'cpu';
  }
  if (!gpu) return null;
  if (gpu.selected_backend === 'ct2_cuda') return 'cuda';
  return gpu.adapters?.length ? 'directml' : 'cpu';
}

/**
 * Card state derived from live data: 'success' means "NVIDIA is ready and not yet in use" (restart when
 * running, used on next start when stopped); 'idle' shows nothing but the offer. In-flight and failed
 * installs keep their own state.
 */
export function nvidiaCardView(gpu, live, state) {
  if (state !== 'idle' && state !== 'success') return state;
  const ready = Boolean(gpu?.nvidia_runtime?.installed && gpu.selected_backend === 'ct2_cuda');
  if (!ready) return 'idle';
  if (!live?.is_running) return state;
  const failed = String(live.backend_reason || '').startsWith('ct2_');
  return live.active_backend === 'ct2_cuda' || failed ? 'idle' : 'success';
}

/** Config to save after installing the runtime so the next start uses it; null keeps an explicit ort choice. */
export function nvidiaInstallConfig(config) {
  if (config.asr_backend === 'ort') return null;
  return { ...config, device: 'cuda', asr_backend: config.asr_backend === 'ct2_cuda' ? 'ct2_cuda' : 'auto' };
}

/** True when the opt-in NVIDIA runtime offer should be shown. */
export function nvidiaOfferVisible(status, dismissed) {
  return Boolean(status && !dismissed
    && status.adapters?.some((adapter) => adapter.vendor === 'nvidia')
    && status.cuda_driver_version >= NVIDIA_MIN_DRIVER
    && status.nvidia_runtime && !status.nvidia_runtime.installed);
}

// The Advanced tab's own settings. Styles, saved styles, vocabulary (prompts, blacklist), audio device,
// language and session files belong to other tabs and are never part of an Advanced reset.
export const ADVANCED_KEYS = Object.freeze([
  'model_size', 'device', 'cpu_threads', 'asr_backend', 'silence_timeout', 'max_chunk_duration', 'vad_speech_pad_ms',
  'vad_threshold', 'rnnoise_enabled', 'live_partial_subtitles_enabled', 'subtitle_backlog_policy',
  'subtitle_max_live_delay_sec', 'subtitle_catchup_interval_sec', 'ws_port', 'acoustic_events_enabled',
  'mcp_style_enabled', 'mcp_system_enabled', 'check_updates_enabled',
]);

// Defaults of the Advanced keys, same as LiveAudioConfig::default (cpu_threads is half the cores).
export function advancedDefaults(cores = 4) {
  return {
    model_size: 'small (Balance CPU)', device: 'cpu', cpu_threads: Math.min(16, Math.max(1, Math.floor(cores / 2))), asr_backend: 'auto',
    silence_timeout: 0.5, max_chunk_duration: 3.0, vad_speech_pad_ms: 200, vad_threshold: 0.5, rnnoise_enabled: false,
    live_partial_subtitles_enabled: false, subtitle_backlog_policy: 'auto', subtitle_max_live_delay_sec: 10.0,
    subtitle_catchup_interval_sec: 1.5, ws_port: 8765, acoustic_events_enabled: false, mcp_style_enabled: false, mcp_system_enabled: false, check_updates_enabled: true,
  };
}

// Footer Reset: the settings each tab owns (i18n label keys, shown in the confirmation) and their defaults.
// Home has nothing resettable and Style has its own Reset style, so neither is listed. Saved styles, language,
// NVIDIA runtime files, usage stats and session files are never part of a tab reset.
export const TAB_RESETS = Object.freeze({
  'tab-subtitles': {
    title: 'tab_subtitles',
    labels: ['lbl_display_mode', 'lbl_ribbon_lines'],
    defaults: () => ({ subtitle_display_mode: 'adaptive', subtitle_ribbon_max_lines: 3 }),
  },
  'tab-advanced': {
    title: 'tab_advanced',
    labels: ['lbl_model_size', 'lbl_compute_device', 'lbl_cpu_threads', 'lbl_asr_backend', 'lbl_silence_timeout', 'lbl_max_chunk', 'lbl_vad_pad',
      'lbl_vad_threshold', 'lbl_rnnoise_enabled', 'lbl_live_partial_subtitles', 'lbl_backlog_policy', 'lbl_max_live_delay', 'lbl_catchup_interval',
      'lbl_ws_port', 'lbl_acoustic_events_enabled', 'lbl_mcp_style_enabled', 'lbl_mcp_system_enabled', 'lbl_check_updates_auto'],
    defaults: advancedDefaults,
  },
  'tab-files': {
    title: 'tab_files',
    labels: ['lbl_continuous', 'lbl_save_jsonl', 'lbl_save_vtt'],
    defaults: () => ({ continuous_session: true, save_transcript_enabled: true, save_vtt_enabled: true }),
  },
});

// Comparable view of the Advanced settings; model_size keeps only its first word (the form stores the long label).
export function advancedView(config) {
  const view = Object.fromEntries(ADVANCED_KEYS.map((key) => [key, config?.[key]]));
  view.model_size = String(view.model_size ?? '').split(' ')[0];
  view.blacklist = String(config?.blacklist ?? '').trim();
  view.prompt_es = String(config?.whisper_context_prompt_es ?? '').trim();
  view.prompt_en = String(config?.whisper_context_prompt_en ?? '').trim();
  return view;
}
