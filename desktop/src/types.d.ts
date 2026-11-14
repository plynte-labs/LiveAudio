// SPDX-License-Identifier: MIT

export interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
  kind: 'input' | 'loopback';
}

export interface ServiceStatus {
  is_running: boolean;
  asr_state: 'stopped' | 'starting' | 'ready' | 'listening' | 'transcribing' | 'degraded' | 'failed';
  vad_onset: boolean;
  ws_port: number;
  ws_clients: number;
  active_device: string | null;
  model_size: string;
  device: string;
  effective_device?: string | null;
  compute_type?: string | null;
  fallback_occurred?: boolean;
  fallback_reason?: string | null;
  active_backend?: string | null;
  backend_reason?: string | null;
  uptime_sec: number;
  restart_count: number;
  session_path: string | null;
  session_error: string | null;
  asr_progress: AsrProgress | null;
  /** Another process owns the running service; this desktop only monitors it. */
  monitor_mode: boolean;
  foreign_service_pid: number | null;
}

export interface AsrProgress {
  phase: string;
  text: string;
  percent: number | null;
  bytes_downloaded: number | null;
  bytes_total: number | null;
  speed_bps: number | null;
  eta_sec: number | null;
  attempt: number | null;
}

export interface RuntimeSetupProgress {
  phase: 'starting' | 'sync' | 'publishing' | 'starting_worker' | 'ready' | 'error';
  text: string;
  percent: number | null;
}

export interface ProfilePreset {
  id: string;
  label: string;
  description: string;
  device: 'cpu' | 'cuda';
  model_size: string;
  silence_timeout: number;
  subtitle_backlog_policy: 'auto' | 'live_only' | 'send_all';
  subtitle_max_live_delay_sec: number;
  subtitle_catchup_interval_sec: number;
}

export interface LiveAudioConfig {
  output_dir: string;
  device: 'cuda' | 'cpu';
  cpu_threads: number;
  model_size: string;
  blacklist: string;
  continuous_session: boolean;
  subtitle_style: 'default' | 'karaoke' | 'neon' | 'minimal' | 'bold' | 'rgb' | 'typewriter';
  subtitle_theme: Record<string, string>;
  subtitle_presets: { name: string; tokens: Record<string, string> }[];
  subtitle_display_mode: 'single' | 'ribbon' | 'adaptive';
  subtitle_ribbon_max_lines: number;
  subtitle_backlog_policy: 'auto' | 'live_only' | 'send_all';
  subtitle_max_live_delay_sec: number;
  subtitle_catchup_interval_sec: number;
  silence_timeout: number;
  max_chunk_duration: number;
  transcription_purpose: 'subtitles' | 'transcription' | 'combined';
  asr_decode_timeout_sec: number;
  audio_device: { id?: string; name?: string } | null;
  selected_profile_id: string;
  profile_mode: 'preset' | 'custom';
  ws_port: number;
  obs_enabled: boolean;
  rnnoise_enabled: boolean;
  mcp_style_enabled: boolean;
  mcp_system_enabled: boolean;
  check_updates_enabled: boolean;
  acoustic_events_enabled: boolean;
  live_partial_subtitles_enabled: boolean;
  asr_backend: string;
  prewarm: boolean;
  save_transcript_enabled: boolean;
  save_vtt_enabled: boolean;
  whisper_context_prompt_es: string;
  whisper_context_prompt_en: string;
  asr_language: 'es' | 'en';
  settings_navigation_mode: 'tabs' | 'dropdown';
  language: string | null;
  diagnostics_enabled: boolean;
  diagnostics_level: 'off' | 'minimal' | 'deep';
  diagnostics_export_dir: string | null;
  last_update_check: number;
  vad_speech_pad_ms: number;
  vad_threshold: number;
  [key: string]: any;
}

export interface ObsOverlayInfo {
  url: string;
  port: number;
  width: number;
  height: number;
  instructions: string;
}

export interface DiagnosticsExportResult {
  path: string;
  timestamp: string;
  success: boolean;
}

export interface SubtitleCue {
  id: number;
  start: number;
  end: number;
  text: string;
  style: string;
  timestamp_ms: number;
}

export interface UpdateMetadata {
  version: string;
  current_version: string;
  notes: string | null;
  pub_date: string | null;
}

export interface UpdaterStatus {
  current_version: string;
  available_update: UpdateMetadata | null;
  install_on_close: boolean;
  is_checking: boolean;
  is_installing: boolean;
  stream_busy: boolean;
  last_check: number;
}

interface GpuBackendStatus {
  adapters: { vendor: string; name: string; dedicated_vram_mb: number }[];
  cuda_driver_version: number | null;
  nvidia_runtime: { installed: boolean; missing: string[] };
  license_urls: string[];
  selected_backend: string;
  reason: string;
}
