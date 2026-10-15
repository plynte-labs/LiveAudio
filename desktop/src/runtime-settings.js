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
];

export function runtimeSettingsChanged(before, after) {
  return RUNTIME_SETTINGS.some((key) => JSON.stringify(before?.[key]) !== JSON.stringify(after?.[key]));
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

export function bindTabNavigation(tablist) {
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
  };

  const initialIndex = Math.max(0, tabs.findIndex((tab) => tab.classList.contains('active')));
  activate(initialIndex);

  tabs.forEach((tab, index) => {
    tab.addEventListener('click', () => activate(index));
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
      activate(nextIndex, true);
    });
  });
}

export function hardwareStatusLabel(status) {
  const runtimeLabel = status.is_running ? 'Activo' : 'Configurado';
  return `${runtimeLabel}: ${status.model_size.split(' ')[0]} | ${status.device.toUpperCase()} | ${status.active_device || 'Default input'}`;
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
