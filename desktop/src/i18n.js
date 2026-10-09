// SPDX-License-Identifier: MIT

/**
 * LiveAudio Desktop Internationalization (i18n) Module
 * Supports bilingual switching (Español / English) with parity to Python v1.2.7.
 */

export const TRANSLATIONS = {
  es: {
    // Header & Brand
    app_title: 'LiveAudio',
    app_version: 'v1.5.0 Tauri 2',
    author_credit: 'by Franguh',
    author_credit_accessible: 'Visitar el perfil de Franguh en GitHub (se abre en el navegador del sistema)',
    toast_author_profile_open_failed: 'No se pudo abrir el perfil de Franguh en el navegador: ',
    btn_minimize: 'Minimizar',
    btn_maximize: 'Maximizar / Restaurar',
    btn_close: 'Cerrar',

    // Status Pills
    status_stopped: 'Detenido',
    status_starting: 'Iniciando...',
    status_ready: 'Listo',
    status_listening: 'Escuchando',
    status_transcribing: 'Transcribiendo',
    status_degraded: 'Degradado',
    status_failed: 'Fallo',
    status_error: 'Error',
    header_details: 'Detalles',
    settings_tabs_label: 'Configuración',
    vad_silence: 'VAD: Silencio',
    vad_speech: 'VAD: Voz Activa',
    ws_clients_suffix: 'cli',

    // Action Bar
    btn_start: 'Iniciar Transcripción',
    btn_stop: 'Detener Transcripción',
    btn_starting: 'Iniciando servicio...',
    btn_stopping: 'Deteniendo...',
    btn_save: 'Guardar',
    btn_reset: 'Restablecer',
    btn_diag: 'Diagnóstico',
    title_save_btn: 'Guardar Cambios',
    title_reset_btn: 'Valores por Defecto',
    title_diag_btn: 'Exportar Reporte Diagnóstico',

    // Tabs
    tab_setup: 'Inicio',
    tab_subtitles: 'Subtítulos',
    tab_advanced: 'Avanzado',
    tab_files: 'Archivos',

    // Tab 1: Audio & Hardware
    lbl_audio_device: 'Fuente de audio',
    opt_default_mic: 'Micrófono predeterminado del sistema',
    opt_loopback: 'Audio del sistema (WASAPI Loopback)',
    hint_audio_device: 'Elige el micrófono o el audio del sistema que quieres transcribir.',
    lbl_compute_device: 'Dispositivo de cómputo',
    opt_cuda: 'CUDA (GPU Nvidia - Recomendado)',
    opt_cpu: 'CPU (Sin GPU dedicada)',
    hint_compute_device: 'CUDA usa la GPU compatible; CPU es una alternativa si no hay GPU disponible.',
    lbl_cuda: 'CUDA (GPU Nvidia - Recomendado)',
    lbl_cpu: 'CPU (Sin GPU dedicada)',
    lbl_cpu_threads: 'Hilos de CPU',
    hint_cpu_threads: 'Número de hilos que se usarán al transcribir con CPU.',
    lbl_silence_timeout: 'Pausa para cerrar una frase',
    hint_silence_timeout: 'Pausa de voz antes de procesar la frase.',
    lbl_max_chunk: 'Duración máxima del segmento',
    hint_max_chunk: 'Límite antes de procesar un segmento de audio.',

    // Tab 2: Model ASR
    lbl_profile: 'Perfil recomendado',
    opt_profile_custom: 'Personalizado',
    opt_profile_fast: 'Rápido',
    opt_profile_balanced: 'Balanceado',
    opt_profile_quality: 'Calidad',
    opt_profile_stable: 'Streaming estable',
    hint_profile: 'Empieza con el perfil recomendado; puedes ajustar cada opción después.',
    profile_desc_custom: 'Configuración personalizada.',
    profile_desc_fast: 'Menos demora y frases cortas; baja un poco la precisión.',
    profile_desc_balanced: 'Recomendado para la mayoría de sesiones.',
    profile_desc_quality: 'Más precisión; puede usar más VRAM y tardar más.',
    profile_desc_stable_streaming: 'Reduce carga de GPU para jugar o transmitir en PC ocupada.',
    profile_desc_modified: 'Perfil modificado: se aplicará como Personalizado.',
    lbl_model_size: 'Tamaño de Modelo Whisper',
    opt_model_tiny: 'tiny (Más rápido, baja precisión)',
    opt_model_base: 'base (Rápido)',
    opt_model_small: 'small (Balance CPU / GPU equilibrado)',
    opt_model_turbo: 'turbo (Máxima precisión GPU)',
    hint_model_size: 'Modelos más grandes aumentan precisión acústica pero consumen más VRAM.',
    lbl_asr_lang: 'Idioma hablado',
    opt_asr_es: 'Español (es)',
    opt_asr_en: 'Inglés (en)',
    hint_asr_lang: 'Selecciona el idioma que se habla en el audio.',
    lbl_prompt: 'Vocabulario y Prompt Inicial',
    lbl_prompt_es: 'Vocabulario y Prompt Inicial (Español)',
    ph_prompt_es: 'Términos técnicos, jerga de juego, nombres propios...',
    hint_prompt_es: 'Ayuda a reducir alucinaciones en español. Ej: \'Stream de gaming, jugamos Minecraft y charlamos con viewers\'.',
    lbl_prompt_en: 'Vocabulario y Prompt Inicial (Inglés)',
    ph_prompt_en: 'Custom gaming terms, names, slang...',
    hint_prompt_en: 'Ayuda a reducir alucinaciones en inglés. Ej: \'Coding stream, Python tutorials, backend development\'.',
    btn_use_preset_prompt: 'Cargar sugerencia',

    // Tab 3: Subtitles OBS
    lbl_subtitles_folder: 'Carpeta de sesión',
    hint_subtitles_folder: 'Aquí se guardan los archivos de sesión según tus opciones.',
    lbl_sub_style: 'Tema de Subtítulos',
    hint_sub_style: 'Estilo visual del subtítulo (tipografía y colores) renderizado en OBS.',
    opt_style_default: 'Default (Limpio con sombra)',
    opt_style_karaoke: 'Karaoke (Palabra por palabra dorado)',
    opt_style_neon: 'Neon Cyberpunk (Cian brillante)',
    opt_style_minimal: 'Minimalista (Sin bordes)',
    opt_style_bold: 'Bold Impact (Mayúsculas oscuras)',
    opt_style_rgb: 'RGB Fade (Suave vertical)',
    opt_style_typewriter: 'Typewriter (Máquina de escribir)',
    lbl_display_mode: 'Modo de Visualización',
    hint_display_mode: 'Adaptive ajusta según longitud; Single muestra última frase; Ribbon apila líneas.',
    opt_mode_adaptive: 'Adaptive (Inteligente)',
    opt_mode_single: 'Single (Línea única)',
    opt_mode_ribbon: 'Ribbon (Pila vertical multilínea)',
    lbl_ribbon_lines: 'Líneas Máximas Ribbon',
    hint_ribbon_lines: 'Máximo de líneas de subtítulos acumuladas simultáneamente en modo Ribbon.',
    lbl_backlog_policy: 'Entrega de subtítulos atrasados',
    hint_backlog_policy: 'Controla cómo se entregan a OBS los subtítulos que se acumulan.',
    opt_backlog_auto: 'Auto (Recomendado)',
    opt_backlog_live_only: 'Solo en Vivo',
    opt_backlog_send_all: 'Enviar Todo',
    lbl_max_live_delay: 'Retraso Máximo en Vivo (Max Live Delay)',
    hint_max_live_delay: 'Tiempo máximo de retraso tolerado antes de descartar subtítulos acumulados.',
    lbl_catchup_interval: 'Intervalo de Recuperación (Catch-up)',
    hint_catchup_interval: 'Pausa entre subtítulos históricos al desahogar la cola acumulada hacia OBS.',

    // Tab 4: Advanced
    lbl_ws_port: 'Puerto local de OBS',
    hint_ws_port: 'Puerto local que usa la conexión WebSocket para OBS.',
    lbl_vad_pad: 'Audio previo a la voz (VAD)',
    hint_vad_pad: 'Audio previo incluido para evitar cortar el inicio de una palabra.',
    lbl_vad_threshold: 'Sensibilidad de voz',
    hint_vad_threshold: 'Sensibilidad para detectar voz. Mayor valor exige voz más clara para activarse.',
    lbl_blacklist: 'Filtro de Frases Alucinadas (Blacklist)',
    hint_blacklist: 'Palabras separadas por comas que se omitirán si Whisper las genera en silencio.',
    lbl_continuous: 'Continuar la sesión entre reinicios',
    hint_continuous: 'Mantiene la misma carpeta de sesión y archivo de subtítulos al reiniciar el servicio.',
    lbl_save_jsonl: 'Guardar transcripción JSONL',
    lbl_save_vtt: 'Guardar subtítulos WebVTT',
    hint_save_sinks: 'Los formatos habilitados se guardan en el dispositivo; ajusta cada opción por separado.',
    lbl_no_session: 'No hay carpeta de sesión activa.',
    btn_open_session: 'Abrir carpeta de sesión',

    // Right Pane: Preview & OBS
    title_preview: 'Vista Previa de Subtítulos',
    btn_test_subtitle: 'Probar Animación',
    sample_preview_text: 'Subtítulos en tiempo real para streamers y creadores con LiveAudio',
    preview_badge: 'Preview 1920x1080',
    title_obs: 'Integración con OBS Studio',
    lbl_obs_url: 'URL de Fuente de Navegador (Browser Source)',
    btn_copy_url: 'Copiar URL',
    btn_copied: '¡Copiado!',
    obs_help_summary: 'Instrucciones de configuración',
    obs_instructions_html: '<strong>Instrucciones para OBS Studio:</strong><br>1. En OBS, haz clic en el botón <strong>+</strong> en Fuentes y selecciona <strong>Navegador</strong> (Browser).<br>2. Desmarca "Archivo local" y pega la URL de arriba en el campo <strong>URL</strong>.<br>3. Ajusta <strong>Ancho: 1920</strong> y <strong>Alto: 1080</strong>, FPS: 60.<br>4. ¡Listo! Los subtítulos se mostrarán en vivo con fondo transparente.',

    // Dialogs & Confirmations
    modal_title_confirm: 'Confirmar Acción',
    modal_confirm: 'Aceptar',
    modal_cancel: 'Cancelar',
    confirm_restart: 'Los cambios requieren reiniciar la transcripción y reconectar OBS. ¿Aplicarlos ahora?',
    confirm_reset: 'Restablecer requiere detener la transcripción y reconectar OBS. ¿Continuar?',
    toast_service_stopped: 'Servicio detenido',
    toast_service_started: 'Servicio iniciado y escuchando audio',
    toast_config_saved_restarted: 'Configuración guardada y aplicada; servicio reiniciado',
    toast_config_saved: 'Configuración guardada exitosamente',
    toast_config_saved_restart_err: 'Configuración guardada, pero el servicio no pudo reiniciarse: ',
    toast_config_reset_restarted: 'Configuración restablecida y aplicada; servicio reiniciado',
    toast_config_reset: 'Configuración restablecida a valores por defecto',
    toast_obs_copied: 'URL de fuente OBS copiada al portapapeles',
    toast_obs_copy_failed: 'No se pudo copiar automáticamente',
    toast_session_opened: 'Carpeta de sesión abierta: ',
    toast_session_open_failed: 'No se pudo abrir la carpeta de sesión: ',
    toast_diag_exported: 'Diagnóstico exportado en: ',
    toast_diag_failed: 'Error al exportar diagnóstico: ',
  },

  en: {
    // Header & Brand
    app_title: 'LiveAudio',
    app_version: 'v1.5.0 Tauri 2',
    author_credit: 'by Franguh',
    author_credit_accessible: 'Visit Franguh on GitHub (opens in the system browser)',
    toast_author_profile_open_failed: 'Could not open Franguh’s profile in the browser: ',
    btn_minimize: 'Minimize',
    btn_maximize: 'Maximize / Restore',
    btn_close: 'Close',

    // Status Pills
    status_stopped: 'Stopped',
    status_starting: 'Starting...',
    status_ready: 'Ready',
    status_listening: 'Listening',
    status_transcribing: 'Transcribing',
    status_degraded: 'Degraded',
    status_failed: 'Failed',
    status_error: 'Error',
    header_details: 'Details',
    settings_tabs_label: 'Settings',
    vad_silence: 'VAD: Silence',
    vad_speech: 'VAD: Active Voice',
    ws_clients_suffix: 'cli',

    // Action Bar
    btn_start: 'Start Transcription',
    btn_stop: 'Stop Transcription',
    btn_starting: 'Starting service...',
    btn_stopping: 'Stopping...',
    btn_save: 'Save',
    btn_reset: 'Reset',
    btn_diag: 'Diagnostics',
    title_save_btn: 'Save Changes',
    title_reset_btn: 'Reset Defaults',
    title_diag_btn: 'Export Diagnostic Report',

    // Tabs
    tab_setup: 'Setup',
    tab_subtitles: 'Subtitles',
    tab_advanced: 'Advanced',
    tab_files: 'Files',

    // Tab 1: Audio & Hardware
    lbl_audio_device: 'Audio source',
    opt_default_mic: 'System Default Microphone',
    opt_loopback: 'System Audio (WASAPI Loopback)',
    hint_audio_device: 'Choose the microphone or system audio you want to transcribe.',
    lbl_compute_device: 'Compute device',
    opt_cuda: 'CUDA (Nvidia GPU - Recommended)',
    opt_cpu: 'CPU (No dedicated GPU)',
    hint_compute_device: 'Use a compatible CUDA GPU, or choose CPU when one is not available.',
    lbl_cuda: 'CUDA (Nvidia GPU - Recommended)',
    lbl_cpu: 'CPU (No dedicated GPU)',
    lbl_cpu_threads: 'CPU threads',
    hint_cpu_threads: 'Number of threads used when transcribing with CPU.',
    lbl_silence_timeout: 'Pause before ending a phrase',
    hint_silence_timeout: 'Voice pause before the phrase is processed.',
    lbl_max_chunk: 'Maximum segment duration',
    hint_max_chunk: 'Limit before an audio segment is processed.',

    // Tab 2: Model ASR
    lbl_profile: 'Recommended profile',
    opt_profile_custom: 'Custom',
    opt_profile_fast: 'Fast',
    opt_profile_balanced: 'Balanced',
    opt_profile_quality: 'Quality',
    opt_profile_stable: 'Stable Streaming',
    hint_profile: 'Start with a recommended profile; adjust individual settings later.',
    profile_desc_custom: 'Custom configuration.',
    profile_desc_fast: 'Lower delivery delay and shorter phrases; slightly reduces precision.',
    profile_desc_balanced: 'Recommended for most sessions.',
    profile_desc_quality: 'More precision; can use more VRAM and take longer.',
    profile_desc_stable_streaming: 'Reduces GPU load for playing or streaming on a busy PC.',
    profile_desc_modified: 'Modified profile: will apply as Custom.',
    lbl_model_size: 'Whisper Model Size',
    opt_model_tiny: 'tiny (Fastest, low precision)',
    opt_model_base: 'base (Fast)',
    opt_model_small: 'small (Balanced CPU / GPU)',
    opt_model_turbo: 'turbo (Maximum GPU precision)',
    hint_model_size: 'Larger models increase transcription accuracy but require more VRAM and compute time.',
    lbl_asr_lang: 'Spoken language',
    opt_asr_es: 'Spanish (es)',
    opt_asr_en: 'English (en)',
    hint_asr_lang: 'Select the language spoken in the audio.',
    lbl_prompt: 'Initial Vocabulary & Context Prompt',
    lbl_prompt_es: 'Initial Prompt & Vocabulary (Spanish)',
    ph_prompt_es: 'Technical terms, gaming slang, proper nouns...',
    hint_prompt_es: 'Helps reduce hallucinations in Spanish. E.g., \'Gaming stream, chatting with viewers\'.',
    lbl_prompt_en: 'Initial Prompt & Vocabulary (English)',
    ph_prompt_en: 'Custom gaming terms, names, slang...',
    hint_prompt_en: 'Helps reduce hallucinations in English. E.g., \'Coding stream, Python tutorials, backend development\'.',
    btn_use_preset_prompt: 'Load suggestion',

    // Tab 3: Subtitles OBS
    lbl_subtitles_folder: 'Session folder',
    hint_subtitles_folder: 'Session files are saved here according to your options.',
    lbl_sub_style: 'Subtitle Theme',
    hint_sub_style: 'Visual typography and color style rendered in the OBS browser overlay.',
    opt_style_default: 'Default (Clean with shadow)',
    opt_style_karaoke: 'Karaoke (Word-by-word golden)',
    opt_style_neon: 'Neon Cyberpunk (Glowing cyan)',
    opt_style_minimal: 'Minimalist (Borderless)',
    opt_style_bold: 'Bold Impact (Dark uppercase)',
    opt_style_rgb: 'RGB Fade (Smooth vertical)',
    opt_style_typewriter: 'Typewriter (Typewriter style)',
    lbl_display_mode: 'Display Mode',
    hint_display_mode: 'Adaptive wraps dynamically; Single shows latest phrase; Ribbon stacks lines.',
    opt_mode_adaptive: 'Adaptive (Smart)',
    opt_mode_single: 'Single (Single line)',
    opt_mode_ribbon: 'Ribbon (Vertical multi-line stack)',
    lbl_ribbon_lines: 'Maximum Ribbon Lines',
    hint_ribbon_lines: 'Maximum subtitle lines accumulated on screen in Ribbon display mode.',
    lbl_backlog_policy: 'Delayed subtitle delivery',
    hint_backlog_policy: 'Controls how accumulated subtitles are delivered to OBS.',
    opt_backlog_auto: 'Auto (Recommended)',
    opt_backlog_live_only: 'Live Only',
    opt_backlog_send_all: 'Send All',
    lbl_max_live_delay: 'Maximum Live Delay',
    hint_max_live_delay: 'Maximum tolerated backlog delay before dropping stale subtitles.',
    lbl_catchup_interval: 'Catch-up Interval',
    hint_catchup_interval: 'Pacing interval between historical subtitles when catching up in OBS.',

    // Tab 4: Advanced
    lbl_ws_port: 'Local OBS port',
    hint_ws_port: 'Local port used by the OBS WebSocket connection.',
    lbl_vad_pad: 'Pre-speech audio (VAD)',
    hint_vad_pad: 'Includes audio before speech to avoid cutting off a word.',
    lbl_vad_threshold: 'Voice sensitivity',
    hint_vad_threshold: 'Higher values require louder speech to activate.',
    lbl_blacklist: 'Hallucination Filter (Blacklist)',
    hint_blacklist: 'Comma-separated words to omit if Whisper hallucinates them during silence.',
    lbl_continuous: 'Continue the session across restarts',
    hint_continuous: 'Reuses the same session directory and files across service restarts.',
    lbl_save_jsonl: 'Save JSONL transcript',
    lbl_save_vtt: 'Save WebVTT subtitles',
    hint_save_sinks: 'Enabled formats are saved on this device; adjust each option separately.',
    lbl_no_session: 'No active session folder.',
    btn_open_session: 'Open session folder',

    // Right Pane: Preview & OBS
    title_preview: 'Subtitle Live Preview',
    btn_test_subtitle: 'Test Animation',
    sample_preview_text: 'Real-time subtitles for streamers and creators with LiveAudio',
    preview_badge: 'Preview 1920x1080',
    title_obs: 'OBS Studio Integration',
    lbl_obs_url: 'Browser Source URL',
    btn_copy_url: 'Copy URL',
    btn_copied: 'Copied!',
    obs_help_summary: 'Setup instructions',
    obs_instructions_html: '<strong>OBS Studio Instructions:</strong><br>1. In OBS, click <strong>+</strong> in Sources and select <strong>Browser</strong>.<br>2. Uncheck "Local file" and paste the URL above into the <strong>URL</strong> field.<br>3. Set <strong>Width: 1920</strong> and <strong>Height: 1080</strong>, FPS: 60.<br>4. Done! Subtitles will display live with a transparent background.',

    // Dialogs & Confirmations
    modal_title_confirm: 'Action Required',
    modal_confirm: 'Confirm',
    modal_cancel: 'Cancel',
    confirm_restart: 'Changes require restarting transcription and reconnecting OBS. Apply them now?',
    confirm_reset: 'Reset requires stopping transcription and reconnecting OBS. Continue?',
    toast_service_stopped: 'Service stopped',
    toast_service_started: 'Service started and listening to audio',
    toast_config_saved_restarted: 'Settings saved and applied; service restarted',
    toast_config_saved: 'Settings saved successfully',
    toast_config_saved_restart_err: 'Settings saved, but service could not restart: ',
    toast_config_reset_restarted: 'Settings reset and applied; service restarted',
    toast_config_reset: 'Settings reset to default values',
    toast_obs_copied: 'OBS source URL copied to clipboard',
    toast_obs_copy_failed: 'Could not copy automatically',
    toast_session_opened: 'Session folder opened: ',
    toast_session_open_failed: 'Could not open session folder: ',
    toast_diag_exported: 'Diagnostics exported to: ',
    toast_diag_failed: 'Error exporting diagnostics: ',
  },
};

/**
 * Recommended prompts and metadata for each transcription profile in ES and EN.
 */
export const PROFILE_I18N = {
  fast: {
    label: { es: 'Rápido', en: 'Fast' },
    description: {
      es: 'Menos demora y frases cortas; baja un poco la precisión.',
      en: 'Lower delivery delay and shorter phrases; slightly reduces precision.',
    },
    samplePrompt: {
      es: 'Transmisión en vivo rápida, chat corto, gameplay dinámico.',
      en: 'Fast-paced live stream, quick chat, dynamic gameplay.',
    },
  },
  balanced: {
    label: { es: 'Balanceado', en: 'Balanced' },
    description: {
      es: 'Recomendado para la mayoría de sesiones.',
      en: 'Recommended for most sessions.',
    },
    samplePrompt: {
      es: 'Transmisión en vivo, charla general, videojuegos y contenido multimedia.',
      en: 'Live streaming, general conversation, gaming, podcasts.',
    },
  },
  quality: {
    label: { es: 'Calidad', en: 'Quality' },
    description: {
      es: 'Más precisión; puede usar más VRAM y tardar más.',
      en: 'More precision; can use more VRAM and take longer.',
    },
    samplePrompt: {
      es: 'Grabación formal, vocabulario técnico preciso, dicción clara, puntuación exacta.',
      en: 'Formal recording, precise technical vocabulary, clear diction, exact punctuation.',
    },
  },
  stable_streaming: {
    label: { es: 'Streaming estable', en: 'Stable Streaming' },
    description: {
      es: 'Reduce carga de GPU para jugar o transmitir en PC ocupada.',
      en: 'Reduces GPU load for playing or streaming on a busy PC.',
    },
    samplePrompt: {
      es: 'Streaming gaming de alta carga gráfica, Discord, multijugador.',
      en: 'Heavy graphic gaming stream, Discord voice, multiplayer gameplay.',
    },
  },
  custom: {
    label: { es: 'Personalizado', en: 'Custom' },
    description: {
      es: 'Configuración personalizada.',
      en: 'Custom configuration.',
    },
    samplePrompt: {
      es: '',
      en: '',
    },
  },
};

let currentLanguage = 'es';

export function getLanguage() {
  return currentLanguage;
}

export function setLanguage(lang) {
  if (lang !== 'es' && lang !== 'en') {
    lang = 'es';
  }
  currentLanguage = lang;
  try {
    localStorage.setItem('liveaudio_ui_lang', lang);
  } catch (_) {}
  applyTranslations(lang);
  return currentLanguage;
}

export function initLanguage(preferredLang) {
  let lang = preferredLang;
  if (!lang) {
    try {
      lang = localStorage.getItem('liveaudio_ui_lang');
    } catch (_) {}
  }
  if (!lang) {
    const navLang = typeof navigator !== 'undefined' ? (navigator.language || '') : '';
    lang = navLang.toLowerCase().startsWith('en') ? 'en' : 'es';
  }
  return setLanguage(lang);
}

export function t(key) {
  const dict = TRANSLATIONS[currentLanguage] || TRANSLATIONS.es;
  return dict[key] !== undefined ? dict[key] : (TRANSLATIONS.es[key] !== undefined ? TRANSLATIONS.es[key] : key);
}

export function getProfilePresetData(profileId, lang = currentLanguage) {
  const profile = PROFILE_I18N[profileId] || PROFILE_I18N.custom;
  return {
    label: profile.label[lang] || profile.label.es,
    description: profile.description[lang] || profile.description.es,
    samplePrompt: profile.samplePrompt[lang] || profile.samplePrompt.es,
  };
}

export function applyTranslations(lang = currentLanguage) {
  const dict = TRANSLATIONS[lang] || TRANSLATIONS.es;
  if (typeof document === 'undefined') return;

  // 1. Text content
  document.querySelectorAll('[data-i18n]').forEach((el) => {
    const key = el.getAttribute('data-i18n');
    if (dict[key] !== undefined) {
      el.textContent = dict[key];
    }
  });

  // 2. HTML content
  document.querySelectorAll('[data-i18n-html]').forEach((el) => {
    const key = el.getAttribute('data-i18n-html');
    if (dict[key] !== undefined) {
      el.innerHTML = dict[key];
    }
  });

  // 3. Placeholders
  document.querySelectorAll('[data-i18n-placeholder]').forEach((el) => {
    const key = el.getAttribute('data-i18n-placeholder');
    if (dict[key] !== undefined) {
      el.placeholder = dict[key];
    }
  });

  // 4. Titles / Tooltips
  document.querySelectorAll('[data-i18n-title]').forEach((el) => {
    const key = el.getAttribute('data-i18n-title');
    if (dict[key] !== undefined) {
      el.title = dict[key];
    }
  });

  // 5. Accessible labels
  document.querySelectorAll('[data-i18n-aria-label]').forEach((el) => {
    const key = el.getAttribute('data-i18n-aria-label');
    if (dict[key] !== undefined) {
      el.setAttribute('aria-label', dict[key]);
    }
  });
}
