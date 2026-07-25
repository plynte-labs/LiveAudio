# SPDX-License-Identifier: MIT
import locale
import os
import sys

_current_lang = "es"

TRANSLATIONS = {
    "es": {
        # Welcome screen
        "welcome_title": "LiveAudio",
        "welcome_by": "by Plynte",
        "tips_text": "💡 Tips de configuración:\n\n• Gaming Pesado (AAA): Usa CPU + Modelo Small.\n• Just Chatting / Gaming Ligero: Usa GPU + Modelo Turbo.\n• Si desconectas tu micrófono, el sistema se reconectará solo.",
        "current_sessions_folder": "Carpeta de Sesiones Actual:",
        "change_folder": "Cambiar Carpeta",
        "settings_layout": "Diseño del Menú de Ajustes:",
        "tabs_horizontal": "Pestañas (Horizontal)",
        "dropdown_menu": "Menú Desplegable",
        "continue": "CONTINUAR",
        "welcome_lang_label": "Idioma / Language:",
        
        # Main screen left panel
        "settings": "Ajustes",
        "settings_section": "Sección de Ajustes:",
        "profiles": "Perfiles",
        "audio_vad": "Audio/VAD",
        "performance": "Rendimiento",
        "subtitles": "Subtítulos",
        "advanced": "Avanzado",
        
        # Profiles Section
        "config_profile": "Perfil de configuración:",
        "custom": "Personalizado",
        "active_config": "Configuración activa",
        "pending_changes_btn": "Cambios pendientes: pulsa Aplicar cambios",
        "modified_profile_desc": "Perfil modificado: se aplicará como Personalizado.",
        "advanced_manual_desc": "Ajustes manuales avanzados.",
        "profile_change_warning_title": "Cambios pendientes",
        "profile_change_warning_msg": "Cambiar de perfil descartará los cambios pendientes. ¿Continuar?",
        
        # Audio/VAD Section
        "audio_device_label": "Dispositivo de Audio:",
        "system_default": "🔄 Por defecto del sistema",
        "latency_control": "Control de Latencia (Ritmo):",
        "silence_detection": "Detección de Silencio: {:.1f}s",
        "max_phrase_duration": "Duración máxima de frase: {:.1f}s",
        "vad_speech_pad": "Pre-roll de inicio de voz: {:.0f}ms",
        "vad_threshold_label": "Sensibilidad del VAD: {:.2f}",
        
        # Performance Section
        "hardware": "Hardware:",
        "cpu_threads": "Hilos CPU (Max {}):",
        "model_size": "Tamaño del Modelo:",
        "spoken_language_label": "Idioma de voz (Habla):",
        "whisper_context": "Contexto para Whisper (opcional):",
        "whisper_context_help": "Ayuda a reducir alucinaciones. Ej: 'Stream de tecnología, el host habla de Python y gaming'.",
        "whisper_context_help_es": "Ayuda a reducir alucinaciones en español. Ej: 'Stream de gaming, jugamos Minecraft y charlamos con viewers'.",
        "whisper_context_help_en": "Ayuda a reducir alucinaciones en inglés. Ej: 'Coding stream, Python tutorials, backend development'.",
        
        # Subtitles Section
        "style_preview": "Preview del estilo:",
        "preview_sample_text": "Hola mundo! Internacionalización y extraordinariamente con emojis 🎤🎵 y mixed content",
        "visual_style": "Estilo Visual:",
        "send_to_obs": "Enviar subtítulos a OBS",
        "how_to_add_obs": "📺 Cómo agregar en OBS:",
        "obs_guide_steps": (
            "1. En OBS, agrega una fuente: «Navegador» (Browser)\n"
            "2. Marca «Archivo local» y selecciona:\n"
            "   {html_path}\n"
            "3. Ancho: 1920 | Alto: 200 (o mas segun estilo)\n"
            "4. En la URL, agrega el puerto si no es 8765:\n"
            "   file:///.../subtitulos_obs.html?port=8765\n"
            "5. Listo! Los subtitulos apareceran en vivo."
        ),
        "save_transcript": "Guardar transcripción (transcript.jsonl)",
        "save_vtt": "Guardar subtítulos (subtitles.vtt)",
        "ws_port_label": "Puerto WebSocket (base):",
        "obs_backlog": "Atraso en OBS:",
        "advanced_sub_config": "▼ Configuración avanzada",
        "hide_advanced_sub_config": "▲ Ocultar configuración avanzada",
        "max_live_delay": "Max atraso live: {:.1f}s",
        "catchup_pacing": "Pacing catch-up: {:.1f}s",
        
        # Advanced Section
        "anti_hallucination": "Filtro Anti-Alucinaciones:",
        "session_persistence": "Mantener la misma sesión al reiniciar motor",
        
        # Main screen bottom controls
        "apply_changes": "APLICAR CAMBIOS",
        "discard_changes": "Descartar cambios",
        "start_system": "INICIAR SISTEMA",
        "stop_system": "DETENER SISTEMA",
        
        # Live Panel Right Side
        "live_panel": "Panel en vivo",
        "privacy_notice": "Privacidad: ASR local. Los subtítulos se guardan en disco y se emiten por WebSocket local.",
        "active_session": "Sesion activa: {}",
        "output_folder": "Carpeta de salida: {}",
        "session_not_started": "Sesion: sin iniciar",
        "session_saving": "Sesion: guardando",
        "show_advanced_logs": "Mostrar logs y diagnostico avanzado",
        "technical_logs": "Logs tecnicos (ultimas 300 lineas)",
        "last_subtitle_sent": "Último subtítulo enviado",
        "no_transcripts_yet": "Sin transcripciones todavía.",
        
        # Dialogs / Popups
        "pending_changes_title": "Cambios pendientes",
        "welcome_pending_msg": "Hay cambios pendientes antes de iniciar.\n\nSí: aplicar e iniciar.\nNo: iniciar sin aplicar.\nCancelar: volver.",
        "closing_pending_msg": "Hay cambios pendientes.\n\nSí: aplicar y cerrar.\nNo: descartar y cerrar.\nCancelar: volver.",
        "profile_modified_title": "Perfil modificado",
        "profile_modified_msg": "Modificaste un perfil integrado.\n\nSí: aplicar como Personalizado.\nNo: descartar cambios.\nCancelar: seguir editando.",
        "hot_swap_confirm_msg": "Algunos cambios requieren hot-swap del motor. Puede haber un corte breve y perderse la frase actual. ¿Aplicar ahora?",
        "error_applying_title": "Error al aplicar",
        "error_applying_msg": "No se pudieron aplicar los cambios:\n{}",
        "ws_port_busy_title": "Puerto en uso",
        "ws_port_busy_msg": "No se puede iniciar el servidor de subtítulos: los puertos {port}-{end_port} están en uso por otras aplicaciones.\n\n• Cierra las aplicaciones que usan ese rango, o\n• Cambia \"ws_port\" en config.json.",

        # Updates
        "update_available_banner": "✨ ¡Nueva versión {} disponible! Haz clic para ver las novedades.",
        "update_now": "Actualizar ahora",
        "update_later": "Más tarde",

        # Status labels
        "status_audio_init": "Audio: iniciando",
        "status_audio_listening": "Audio: escuchando",
        "status_audio_ready": "Audio: listo",
        "status_audio_stopped": "Audio: detenido",
        "status_audio_reconnecting": "Audio: reconectando",
        "status_audio_error": "Audio: error",
        "status_audio_reconnecting_warn": "Audio: reconectando",
        "status_audio_disconnect": "Audio: desconectado",
        "status_vad_loading": "VAD: cargando",
        "status_vad_idle": "VAD: inactivo",
        "status_vad_ready": "VAD: listo",
        "status_vad_error": "VAD: error de carga",
        "status_vad_voice": "VAD: voz detectada",
        "status_vad_sending": "VAD: enviando frase",
        "status_vad_sent": "VAD: frase enviada",
        "status_vad_queue_full": "VAD: cola llena",
        "status_vad_applying": "VAD: aplicando cambios",
        "status_asr_loading": "ASR: cargando",
        "status_asr_ready": "ASR: listo",
        "status_asr_stopped": "ASR: detenido",
        "status_asr_applying": "ASR: aplicando cambios",
        "status_ws_initiating": "WS: iniciando",
        "status_ws_stopped": "WS: detenido",
        "status_ws_error": "WS: error",
        "status_ws_backpressure": "WS: backpressure",
        "status_ws_port_busy": "WS: puerto ocupado",
        "status_ws_dead": "WS: caído",
        "status_ws_fallback": "WS: localhost:{port} (respaldo)",
        "status_obs_origin_rejected": "OBS: origen rechazado",
        "obs_guide_port_note": "Puerto activo: {port} (el {base} estaba ocupado). Usa ?port={port} en la URL de OBS.",
        "obs_guide_port_changed": "⚠ El puerto base cambió. Actualiza la URL de tu fuente de navegador en OBS a:\n{url}\ny recárgala (clic derecho ▸ Actualizar).",
        "ws_port_changed_title": "Puerto de OBS cambiado",
        "ws_port_changed_msg": "El puerto base ahora es {port}.\n\nTu fuente de navegador en OBS sigue apuntando al puerto anterior y dejará de recibir subtítulos.\n\nActualiza su URL a:\n{url}\n\nLuego recárgala en OBS (clic derecho ▸ Actualizar).",
        "log_ws_port_changed": "[Sistema] ⚠ Puerto base cambiado a {port}. Actualiza la URL de OBS a: {url}",
        
        # Profiles Preset texts
        "profile_preset_fast_label": "Rápido",
        "profile_preset_fast_desc": "Menos demora y frases cortas; baja un poco la precisión.",
        "profile_preset_balanced_label": "Balanceado",
        "profile_preset_balanced_desc": "Recomendado para la mayoría de sesiones.",
        "profile_preset_quality_label": "Calidad",
        "profile_preset_quality_desc": "Más precisión; puede usar más VRAM y tardar más.",
        "profile_preset_stable_label": "Streaming estable",
        "profile_preset_stable_desc": "Reduce carga de GPU para jugar o transmitir en PC ocupada.",
        
        # System Logs
        "log_base_init": "[Sistema] Iniciando servicios base...",
        "log_base_stopping": "[Sistema] Apagando todos los servicios...",
        "log_audio_devices_detected": "[Sistema] 🔍 {} dispositivos de audio detectados.",
        "log_config_applied": "[Sistema] Configuración aplicada y guardada.",
        "log_config_error": "[Sistema] Error al aplicar cambios: {}",
        "log_hot_swap": "\n[Sistema] Aplicando cambios con hot-swap en vivo...",
        "log_hot_swap_failed": "El hot-swap no pudo arrancar los procesos de audio/ASR.",
        "log_ws_port_busy": "[WebSocket] Los puertos {port}-{end_port} están en uso. Cierra esas aplicaciones o cambia \"ws_port\" en config.json.",
        "log_ws_dead": "[WebSocket] El servidor de subtítulos se detuvo inesperadamente. Los subtítulos NO están llegando a OBS. Detén y vuelve a iniciar el sistema.",
        
        # Backlog Policy Labels
        "backlog_auto": "Auto (recomendado)",
        "backlog_live_only": "Solo en vivo",
        "backlog_send_all": "Enviar todo",
    },
    "en": {
        # Welcome screen
        "welcome_title": "LiveAudio",
        "welcome_by": "by Plynte",
        "tips_text": "💡 Configuration Tips:\n\n• Heavy Gaming (AAA): Use CPU + Small Model.\n• Just Chatting / Light Gaming: Use GPU + Turbo Model.\n• If you disconnect your microphone, the system will reconnect automatically.",
        "current_sessions_folder": "Current Sessions Folder:",
        "change_folder": "Change Folder",
        "settings_layout": "Settings Menu Layout:",
        "tabs_horizontal": "Tabs (Horizontal)",
        "dropdown_menu": "Dropdown Menu",
        "continue": "CONTINUE",
        "welcome_lang_label": "Language / Idioma:",
        
        # Main screen left panel
        "settings": "Settings",
        "settings_section": "Settings Section:",
        "profiles": "Profiles",
        "audio_vad": "Audio/VAD",
        "performance": "Performance",
        "subtitles": "Subtitles",
        "advanced": "Advanced",
        
        # Profiles Section
        "config_profile": "Configuration Profile:",
        "custom": "Custom",
        "active_config": "Active Configuration",
        "pending_changes_btn": "Pending changes: click Apply Changes",
        "modified_profile_desc": "Modified profile: will be applied as Custom.",
        "advanced_manual_desc": "Advanced manual settings.",
        "profile_change_warning_title": "Pending changes",
        "profile_change_warning_msg": "Changing profiles will discard pending changes. Continue?",
        
        # Audio/VAD Section
        "audio_device_label": "Audio Device:",
        "system_default": "🔄 System Default",
        "latency_control": "Latency Control (Pacing):",
        "silence_detection": "Silence Detection: {:.1f}s",
        "max_phrase_duration": "Max phrase duration: {:.1f}s",
        "vad_speech_pad": "Speech onset pre-roll: {:.0f}ms",
        "vad_threshold_label": "VAD sensitivity: {:.2f}",
        
        # Performance Section
        "hardware": "Hardware:",
        "cpu_threads": "CPU Threads (Max {}):",
        "model_size": "Model Size:",
        "spoken_language_label": "Spoken Language (ASR):",
        "whisper_context": "Context for Whisper (optional):",
        "whisper_context_help": "Helps reduce hallucinations. E.g.: 'Technology stream, the host talks about Python and gaming'.",
        "whisper_context_help_es": "Helps reduce hallucinations in Spanish. E.g.: 'Gaming stream, Minecraft gameplay, chat with viewers'.",
        "whisper_context_help_en": "Helps reduce hallucinations in English. E.g.: 'Coding stream, Python tutorials, backend development'.",
        
        # Subtitles Section
        "style_preview": "Style Preview:",
        "preview_sample_text": "Hello world! Internationalization and extraordinarily with emojis 🎤🎵 and mixed content",
        "visual_style": "Visual Style:",
        "send_to_obs": "Send subtitles to OBS",
        "how_to_add_obs": "📺 How to add in OBS:",
        "obs_guide_steps": (
            "1. In OBS, add a source: «Browser»\n"
            "2. Check «Local file» and select:\n"
            "   {html_path}\n"
            "3. Width: 1920 | Height: 200 (or more depending on style)\n"
            "4. In the URL, add the port if not 8765:\n"
            "   file:///.../subtitulos_obs.html?port=8765\n"
            "5. Ready! The subtitles will appear live."
        ),
        "save_transcript": "Save transcript (transcript.jsonl)",
        "save_vtt": "Save subtitles (subtitles.vtt)",
        "ws_port_label": "WebSocket port (base):",
        "obs_backlog": "OBS Backlog:",
        "advanced_sub_config": "▼ Advanced Configuration",
        "hide_advanced_sub_config": "▲ Hide Advanced Configuration",
        "max_live_delay": "Max live delay: {:.1f}s",
        "catchup_pacing": "Catch-up pacing: {:.1f}s",
        
        # Advanced Section
        "anti_hallucination": "Anti-Hallucination Filter:",
        "session_persistence": "Keep same session when restarting engine",
        
        # Main screen bottom controls
        "apply_changes": "APPLY CHANGES",
        "discard_changes": "Discard changes",
        "start_system": "START SYSTEM",
        "stop_system": "STOP SYSTEM",
        
        # Live Panel Right Side
        "live_panel": "Live Panel",
        "privacy_notice": "Privacy: Local ASR. Subtitles are saved to disk and emitted via local WebSocket.",
        "active_session": "Active session: {}",
        "output_folder": "Output folder: {}",
        "session_not_started": "Session: not started",
        "session_saving": "Session: saving",
        "show_advanced_logs": "Show logs and advanced diagnostics",
        "technical_logs": "Technical logs (last 300 lines)",
        "last_subtitle_sent": "Last subtitle sent",
        "no_transcripts_yet": "No transcripts yet.",
        
        # Dialogs / Popups
        "pending_changes_title": "Pending changes",
        "welcome_pending_msg": "There are pending changes before starting.\n\nYes: apply and start.\nNo: start without applying.\nCancel: go back.",
        "closing_pending_msg": "There are pending changes.\n\nYes: apply and close.\nNo: discard and close.\nCancel: go back.",
        "profile_modified_title": "Profile modified",
        "profile_modified_msg": "You modified an integrated profile.\n\nYes: apply as Custom.\nNo: discard changes.\nCancel: keep editing.",
        "hot_swap_confirm_msg": "Some changes require engine hot-swap. There may be a brief interruption and current phrase might be lost. Apply now?",
        "error_applying_title": "Error applying",
        "error_applying_msg": "Could not apply changes:\n{}",
        "ws_port_busy_title": "Port in use",
        "ws_port_busy_msg": "Cannot start the subtitle server: ports {port}-{end_port} are already in use by other applications.\n\n• Close the applications using that range, or\n• Change \"ws_port\" in config.json.",

        # Updates
        "update_available_banner": "✨ New version {} available! Click to see what's new.",
        "update_now": "Update now",
        "update_later": "Later",

        # Status labels
        "status_audio_init": "Audio: initiating",
        "status_audio_listening": "Audio: listening",
        "status_audio_ready": "Audio: ready",
        "status_audio_stopped": "Audio: stopped",
        "status_audio_reconnecting": "Audio: reconnecting",
        "status_audio_error": "Audio: error",
        "status_audio_reconnecting_warn": "Audio: reconnecting",
        "status_audio_disconnect": "Audio: disconnected",
        "status_vad_loading": "VAD: loading",
        "status_vad_idle": "VAD: inactive",
        "status_vad_ready": "VAD: ready",
        "status_vad_error": "VAD: loading error",
        "status_vad_voice": "VAD: voice detected",
        "status_vad_sending": "VAD: sending phrase",
        "status_vad_sent": "VAD: phrase sent",
        "status_vad_queue_full": "VAD: queue full",
        "status_vad_applying": "VAD: applying changes",
        "status_asr_loading": "ASR: loading",
        "status_asr_ready": "ASR: ready",
        "status_asr_stopped": "ASR: stopped",
        "status_asr_applying": "ASR: applying changes",
        "status_ws_initiating": "WS: initiating",
        "status_ws_stopped": "WS: stopped",
        "status_ws_error": "WS: error",
        "status_ws_backpressure": "WS: backpressure",
        "status_ws_port_busy": "WS: port busy",
        "status_ws_dead": "WS: down",
        "status_ws_fallback": "WS: localhost:{port} (fallback)",
        "status_obs_origin_rejected": "OBS: origin rejected",
        "obs_guide_port_note": "Active port: {port} ({base} was busy). Use ?port={port} in the OBS URL.",
        "obs_guide_port_changed": "⚠ The base port changed. Update your OBS browser source URL to:\n{url}\nthen refresh it (right-click ▸ Refresh).",
        "ws_port_changed_title": "OBS port changed",
        "ws_port_changed_msg": "The base port is now {port}.\n\nYour OBS browser source still points at the previous port and will stop receiving subtitles.\n\nUpdate its URL to:\n{url}\n\nThen refresh it in OBS (right-click ▸ Refresh).",
        "log_ws_port_changed": "[System] ⚠ Base port changed to {port}. Update the OBS URL to: {url}",
        
        # Profiles Preset texts
        "profile_preset_fast_label": "Fast",
        "profile_preset_fast_desc": "Less delay and shorter phrases; slightly reduces precision.",
        "profile_preset_balanced_label": "Balanced",
        "profile_preset_balanced_desc": "Recommended for most sessions.",
        "profile_preset_quality_label": "Quality",
        "profile_preset_quality_desc": "More precision; can use more VRAM and take longer.",
        "profile_preset_stable_label": "Stable streaming",
        "profile_preset_stable_desc": "Reduces GPU load for playing or streaming on a busy PC.",
        
        # System Logs
        "log_base_init": "[System] Initiating base services...",
        "log_base_stopping": "[System] Shutting down all services...",
        "log_audio_devices_detected": "[System] 🔍 {} audio devices detected.",
        "log_config_applied": "[System] Configuration applied and saved.",
        "log_config_error": "[System] Error applying changes: {}",
        "log_hot_swap": "\n[System] Applying changes with live hot-swap...",
        "log_hot_swap_failed": "Hot-swap could not start audio/ASR processes.",
        "log_ws_port_busy": "[WebSocket] Ports {port}-{end_port} are in use. Close those applications or change \"ws_port\" in config.json.",
        "log_ws_dead": "[WebSocket] The subtitle server stopped unexpectedly. Subtitles are NOT reaching OBS. Stop and start the system again.",
        
        # Backlog Policy Labels
        "backlog_auto": "Auto (recommended)",
        "backlog_live_only": "Live only",
        "backlog_send_all": "Send all",
    }
}


def autodetect_language():
    """Autodetecta el lenguaje del sistema. Retorna 'es' o 'en'."""
    try:
        system_locale = locale.getdefaultlocale()[0]
        if system_locale and system_locale.lower().startswith("es"):
            return "es"
    except Exception:
        pass
    return "en"


def get_language():
    return _current_lang


def set_language(lang):
    global _current_lang
    if lang in TRANSLATIONS:
        _current_lang = lang


def t(key, *args, **kwargs):
    """Retorna la cadena traducida para la clave dada."""
    lang = get_language()
    trans = TRANSLATIONS.get(lang, TRANSLATIONS["en"])
    val = trans.get(key, TRANSLATIONS["en"].get(key, key))
    if args or kwargs:
        return val.format(*args, **kwargs)
    return val
