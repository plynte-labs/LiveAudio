# SPDX-License-Identifier: GPL-3.0-or-later
import os
import queue
import datetime
import copy
import multiprocessing as mp
import customtkinter as ctk
import torch
from tkinter import filedialog, messagebox

torch_lib_path = os.path.join(os.path.dirname(torch.__file__), "lib")
os.environ["PATH"] = f"{torch_lib_path};{os.environ.get('PATH', '')}"

from utils.config import load_config, save_config
from core.engine import asr_consumer
from core.audio import audio_producer, list_audio_devices
from core.network import run_ws_server

ctk.set_appearance_mode("dark")
ctk.set_default_color_theme("green")

# Límite de colas IPC para prevenir OOM en sesiones largas
QUEUE_MAXSIZE = 100
LOG_MAX_LINES = 300
PREVIEW_MAX_CHARS = 600
PREVIEW_SAMPLE_TEXT = "Hola mundo! Internacionalización y extraordinariamente con emojis 🎤🎵 y mixed content"
BACKLOG_POLICY_LABELS = {
    "Auto (recomendado)": "auto",
    "Solo en vivo": "live_only",
    "Enviar todo": "send_all",
}
BACKLOG_POLICY_BY_VALUE = {value: label for label, value in BACKLOG_POLICY_LABELS.items()}

# CTk-compatible style properties for subtitle preview panel
PRESET_STYLES = {
    "default": {
        "fg_color": "#1f2a2d",
        "text_color": "#E8E8E8",
        "font_weight": "normal",
        "corner_radius": 8,
        "font_family": "Consolas",
    },
    "karaoke": {
        "fg_color": "#1a1a2e",
        "text_color": "#FFD700",
        "font_weight": "bold",
        "corner_radius": 12,
        "font_family": "Arial",
    },
    "neon": {
        "fg_color": "#0d0d0d",
        "text_color": "#00FF41",
        "font_weight": "bold",
        "corner_radius": 4,
        "font_family": "Consolas",
    },
    "minimal": {
        "fg_color": "transparent",
        "text_color": "#CCCCCC",
        "font_weight": "normal",
        "corner_radius": 0,
        "font_family": "Segoe UI",
    },
    "bold": {
        "fg_color": "#2c2c2c",
        "text_color": "#FFFFFF",
        "font_weight": "bold",
        "corner_radius": 6,
        "font_family": "Arial",
    },
    "rgb": {
        "fg_color": "#1a1a1a",
        "text_color": "#FF6B6B",
        "font_weight": "bold",
        "corner_radius": 10,
        "font_family": "Consolas",
    },
    "typewriter": {
        "fg_color": "#f5f5dc",
        "text_color": "#333333",
        "font_weight": "normal",
        "corner_radius": 2,
        "font_family": "Consolas",
    },
}


def _validate_output_dir(path):
    """Validate that output_dir is writable. Returns (is_valid, error_message)."""
    if not path:
        return False, "La carpeta de salida no esta configurada."
    if not os.path.exists(path):
        return False, f"La carpeta no existe: {path}"
    if not os.path.isdir(path):
        return False, f"La ruta no es una carpeta: {path}"
    try:
        test_file = os.path.join(path, ".liveaudio_write_test")
        with open(test_file, "w") as f:
            f.write("test")
        os.remove(test_file)
    except PermissionError:
        return False, f"No se puede escribir en: {path}. Verifica permisos."
    except OSError as e:
        return False, f"Error al escribir en {path}: {e}"
    return True, ""


PROFILE_PRESETS = {
    "fast": {
        "label": "Rápido",
        "description": "Menos demora y frases cortas; baja un poco la precisión.",
        "values": {
            "device": "cpu",
            "model_size": "base (Rápido)",
            "silence_timeout": 0.4,
            "max_chunk_duration": 3.0,
            "subtitle_backlog_policy": "live_only",
            "subtitle_max_live_delay_sec": 5.0,
            "subtitle_catchup_interval_sec": 0.8,
        },
    },
    "balanced": {
        "label": "Balanceado",
        "description": "Recomendado para la mayoría de sesiones.",
        "values": {
            "device": "cuda",
            "model_size": "small (Balance CPU)",
            "silence_timeout": 0.8,
            "max_chunk_duration": 5.0,
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 10.0,
            "subtitle_catchup_interval_sec": 1.5,
        },
    },
    "quality": {
        "label": "Calidad",
        "description": "Más precisión; puede usar más VRAM y tardar más.",
        "values": {
            "device": "cuda",
            "model_size": "turbo (Máxima precisión GPU)",
            "silence_timeout": 1.0,
            "max_chunk_duration": 8.0,
            "subtitle_backlog_policy": "auto",
            "subtitle_max_live_delay_sec": 15.0,
            "subtitle_catchup_interval_sec": 2.0,
        },
    },
    "stable_streaming": {
        "label": "Streaming estable",
        "description": "Reduce carga de GPU para jugar o transmitir en PC ocupada.",
        "values": {
            "device": "cpu",
            "model_size": "small (Balance CPU)",
            "silence_timeout": 0.6,
            "max_chunk_duration": 4.0,
            "subtitle_backlog_policy": "live_only",
            "subtitle_max_live_delay_sec": 6.0,
            "subtitle_catchup_interval_sec": 1.0,
        },
    },
}
PROFILE_LABEL_TO_ID = {profile["label"]: profile_id for profile_id, profile in PROFILE_PRESETS.items()}


class LiveASRApp(ctk.CTk):
    def __init__(self):
        super().__init__()
        self.title("Plynte LiveAudio")
        self.geometry("1200x800")
        
        # --- DEFENSA 1: Manejo del botón 'X' de Windows ---
        self.protocol("WM_DELETE_WINDOW", self.on_closing)
        
        self.config_data = load_config()
        self.draft_config = copy.deepcopy(self.config_data)
        self._ui_ready = False
        self._applying_settings = False
        self.is_running = False
        
        # --- DEFENSA 2: Memoria Compartida ---
        self.manager = mp.Manager()
        self.shared_config = self.manager.dict(self.config_data)
        
        # Colas IPC con límite de tamaño para prevenir OOM
        self.audio_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)
        self.text_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)
        self.log_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)
        self.p_audio = self.p_ia = self.p_ws = None
        self.current_session_dir = None
        self._log_lines = []
        self._advanced_visible = False
        self.status_labels = {}

        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(0, weight=1)
        self.screen_welcome = ctk.CTkFrame(self, fg_color="transparent")
        self.screen_main = ctk.CTkFrame(self, fg_color="transparent")

        self.build_welcome_screen()
        self.build_main_screen()
        self.screen_welcome.grid(row=0, column=0, sticky="nsew")
        self.after(100, self.process_logs)

    # --- PANTALLA 1: BIENVENIDA ---
    def build_welcome_screen(self):
        container = ctk.CTkFrame(self.screen_welcome, width=600, height=400)
        container.place(relx=0.5, rely=0.5, anchor="center")

        ctk.CTkLabel(container, text="Bienvenido a LiveAudio", font=ctk.CTkFont(size=28, weight="bold")).pack(pady=(30, 10))
        
        tips_text = (
            "💡 Tips de configuración:\n\n"
            "• Gaming Pesado (AAA): Usa CPU + Modelo Small.\n"
            "• Just Chatting / Gaming Ligero: Usa GPU + Modelo Turbo.\n"
            "• Si desconectas tu micrófono, el sistema se reconectará solo."
        )
        ctk.CTkLabel(container, text=tips_text, justify="left", font=ctk.CTkFont(size=14)).pack(pady=20, padx=40)

        ctk.CTkLabel(container, text="Carpeta de Sesiones Actual:", font=ctk.CTkFont(weight="bold")).pack(pady=(10, 0))
        
        self.lbl_folder = ctk.CTkLabel(container, text=self.config_data["output_dir"], text_color="gray")
        self.lbl_folder.pack(pady=5)

        btn_folder = ctk.CTkButton(container, text="Cambiar Carpeta", fg_color="transparent", border_width=1, command=self.change_folder)
        btn_folder.pack(pady=10)

        btn_continue = ctk.CTkButton(container, text="CONTINUAR", height=45, font=ctk.CTkFont(weight="bold"), command=self.go_to_main)
        btn_continue.pack(pady=(30, 30))

    def change_folder(self):
        folder = filedialog.askdirectory(title="Selecciona dónde guardar las sesiones")
        if folder:
            self.config_data["output_dir"] = folder
            self.lbl_folder.configure(text=folder)
            save_config(self.config_data)
            self.shared_config["output_dir"] = folder
            if hasattr(self, "lbl_session"):
                self.update_session_label()

    def go_to_main(self):
        self.screen_welcome.grid_forget() # Ocultar bienvenida
        self.screen_main.grid(row=0, column=0, sticky="nsew") # Mostrar principal

    # --- PANTALLA 2: MOTOR ASR ---
    def build_main_screen(self):
        self.screen_main.grid_columnconfigure(1, weight=1)
        self.screen_main.grid_rowconfigure(0, weight=1)

        # Panel Izquierdo (Ajustes) — con scroll para manejar muchos controles
        frame_izq = ctk.CTkScrollableFrame(self.screen_main, width=320, corner_radius=0)
        frame_izq.grid(row=0, column=0, sticky="nsew")
        
        ctk.CTkLabel(frame_izq, text="Ajustes", font=ctk.CTkFont(size=20, weight="bold")).pack(pady=20, padx=20)

        tabs = ctk.CTkTabview(frame_izq)
        tabs.pack(fill="both", expand=True, padx=10, pady=(0, 10))
        tab_profiles = tabs.add("Perfiles")
        tab_audio = tabs.add("Audio/VAD")
        tab_model = tabs.add("Rendimiento")
        tab_subtitulos = tabs.add("Subtítulos")
        tab_advanced = tabs.add("Avanzado")

        # === SECCIÓN: Perfiles ===
        ctk.CTkLabel(tab_profiles, text="Perfil de configuración:", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        current_profile_id = self.config_data.get("selected_profile_id", "custom")
        current_profile_label = PROFILE_PRESETS.get(current_profile_id, {}).get("label", "Personalizado")
        profile_values = [profile["label"] for profile in PROFILE_PRESETS.values()] + ["Personalizado"]
        self.var_profile = ctk.StringVar(value=current_profile_label)
        self.opt_profile = ctk.CTkOptionMenu(tab_profiles, values=profile_values, variable=self.var_profile, command=self.on_profile_select)
        self.opt_profile.pack(fill="x", padx=10, pady=(0, 6))
        self.lbl_profile_desc = ctk.CTkLabel(tab_profiles, text="", justify="left", wraplength=240, text_color="#AEB8BC")
        self.lbl_profile_desc.pack(fill="x", padx=10, pady=(0, 8))
        self.lbl_pending = ctk.CTkLabel(tab_profiles, text="Configuración activa", text_color="#8BC34A")
        self.lbl_pending.pack(fill="x", padx=10, pady=(0, 12))

        # === SECCIÓN: Dispositivo de Audio ===
        ctk.CTkLabel(tab_audio, text="Dispositivo de Audio:", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        
        # Frame para el selector + botón refresh
        frame_device = ctk.CTkFrame(tab_audio, fg_color="transparent")
        frame_device.pack(fill="x", padx=10, pady=(0, 5))
        frame_device.grid_columnconfigure(0, weight=1)
        
        self._audio_devices = []
        self._device_display_list = ["🔄 Por defecto del sistema"]
        self._refresh_device_list()
        
        # Determinar selección actual
        current_device_display = self._device_display_list[0]
        saved_device = self.config_data.get("audio_device")
        if saved_device and isinstance(saved_device, dict):
            saved_name = saved_device.get("display", "")
            if saved_name in self._device_display_list:
                current_device_display = saved_name
        
        self.var_device = ctk.StringVar(value=current_device_display)
        self.opt_device = ctk.CTkOptionMenu(
            frame_device, 
            values=self._device_display_list, 
            variable=self.var_device, 
            command=self.on_device_change,
            width=240
        )
        self.opt_device.grid(row=0, column=0, sticky="ew")
        
        btn_refresh = ctk.CTkButton(
            frame_device, text="⟳", width=36, 
            command=self._on_refresh_devices,
            fg_color="transparent", border_width=1
        )
        btn_refresh.grid(row=0, column=1, padx=(5, 0))

        # === SECCIÓN: Hardware ===
        ctk.CTkLabel(tab_model, text="Hardware:", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        self.var_hw = ctk.StringVar(value=self.config_data["device"])
        self.opt_hw = ctk.CTkSegmentedButton(tab_model, values=["cpu", "cuda"], variable=self.var_hw, command=self.on_setting_change)
        self.opt_hw.pack(fill="x", padx=10, pady=(0, 15))

        # CPU Threads
        total_cores = mp.cpu_count()
        ctk.CTkLabel(tab_model, text=f"Hilos CPU (Max {total_cores}):").pack(anchor="w", padx=10)
        self.slider_threads = ctk.CTkSlider(tab_model, from_=1, to=total_cores, number_of_steps=total_cores-1, command=self.on_setting_change)
        self.slider_threads.set(self.config_data["cpu_threads"])
        self.slider_threads.pack(fill="x", padx=10, pady=(0, 15))

        # Modelo Selector (Nombres Descriptivos)
        modelos_desc = [
            "tiny (Más rápido, baja precisión)", 
            "base (Rápido)", 
            "small (Balance CPU)", 
            "turbo (Máxima precisión GPU)"
        ]
        # Asegurar que el modelo guardado exista en la lista visual
        current_model = next((m for m in modelos_desc if m.startswith(self.config_data["model_size"].split()[0])), modelos_desc[2])
        
        ctk.CTkLabel(tab_model, text="Tamaño del Modelo:").pack(anchor="w", padx=10)
        self.var_model = ctk.StringVar(value=current_model)
        self.opt_model = ctk.CTkOptionMenu(tab_model, values=modelos_desc, variable=self.var_model, command=self.on_setting_change)
        self.opt_model.pack(fill="x", padx=10, pady=(0, 15))

        # Contexto para Whisper (initial_prompt)
        ctk.CTkLabel(tab_model, text="Contexto para Whisper (opcional):").pack(anchor="w", padx=10)
        self.text_whisper_prompt = ctk.CTkTextbox(tab_model, height=60)
        self.text_whisper_prompt.insert("0.0", self.config_data.get("whisper_context_prompt", ""))
        self.text_whisper_prompt.pack(fill="x", padx=10, pady=(0, 15))
        self.text_whisper_prompt.bind("<FocusOut>", lambda e: self.on_setting_change())
        ctk.CTkLabel(
            tab_model,
            text="Ayuda a reducir alucinaciones. Ej: 'Stream de tecnología, el host habla de Python y gaming'.",
            wraplength=260,
            justify="left",
            font=ctk.CTkFont(size=10),
            text_color="#AEB8BC",
        ).pack(anchor="w", padx=10, pady=(0, 0))

        # --- Sliders de Latencia ---
        ctk.CTkLabel(tab_audio, text="Control de Latencia (Ritmo):", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        
        # Slider Silencio
        self.lbl_silence = ctk.CTkLabel(tab_audio, text=f"Detección de Silencio: {self.config_data['silence_timeout']}s")
        self.lbl_silence.pack(anchor="w", padx=10)
        self.slider_silence = ctk.CTkSlider(tab_audio, from_=0.3, to=2.0, command=self.on_setting_change)
        self.slider_silence.set(self.config_data["silence_timeout"])
        self.slider_silence.pack(fill="x", padx=10, pady=(0, 10))

        # Slider Guillotina
        self.lbl_max_dur = ctk.CTkLabel(tab_audio, text=f"Duración máxima de frase: {self.config_data['max_chunk_duration']}s")
        self.lbl_max_dur.pack(anchor="w", padx=10)
        self.slider_max_dur = ctk.CTkSlider(tab_audio, from_=2.0, to=15.0, command=self.on_setting_change)
        self.slider_max_dur.set(self.config_data["max_chunk_duration"])
        self.slider_max_dur.pack(fill="x", padx=10, pady=(0, 15))

        # Toggle de Sesión
        self.var_session = ctk.BooleanVar(value=self.config_data["continuous_session"])
        self.check_session = ctk.CTkSwitch(tab_advanced, text="Mantener la misma sesión al reiniciar motor", variable=self.var_session, command=self.on_setting_change)
        self.check_session.pack(anchor="w", padx=10, pady=(10, 15))

        # === TAB: Subtítulos ===

        # --- Sección: Preview ---
        ctk.CTkLabel(tab_subtitulos, text="Preview del estilo:", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        self.frame_preview = ctk.CTkFrame(tab_subtitulos, corner_radius=8)
        self.frame_preview.pack(fill="x", padx=10, pady=(0, 8))
        self.lbl_preview_sample = ctk.CTkLabel(
            self.frame_preview,
            text=PREVIEW_SAMPLE_TEXT,
            wraplength=260,
            font=ctk.CTkFont(size=16, weight="normal"),
        )
        self.lbl_preview_sample.pack(padx=12, pady=10)

        # Style selector — MUST be before _update_preview()
        ctk.CTkLabel(tab_subtitulos, text="Estilo Visual:").pack(anchor="w", padx=10, pady=(10, 0))
        self.var_style = ctk.StringVar(value=self.config_data.get("subtitle_style", "default"))
        self.opt_style = ctk.CTkOptionMenu(
            tab_subtitulos,
            values=["default", "karaoke", "neon", "minimal", "bold", "rgb", "typewriter"],
            variable=self.var_style,
            command=self._on_style_change
        )
        self.opt_style.pack(fill="x", padx=10, pady=(0, 8))

        self._update_preview()

        # OBS enable/disable toggle — debajo del preview
        self.var_obs_enabled = ctk.BooleanVar(value=self.config_data.get("obs_enabled", True))
        self.switch_obs_enabled = ctk.CTkSwitch(
            tab_subtitulos,
            text="Enviar subtítulos a OBS",
            variable=self.var_obs_enabled,
            command=self._on_obs_toggle,
        )
        self.switch_obs_enabled.pack(anchor="w", padx=10, pady=(8, 10))

        # Mini guía de OBS
        self.frame_obs_guide = ctk.CTkFrame(tab_subtitulos, fg_color="#1a2730", corner_radius=8)
        self.frame_obs_guide.pack(fill="x", padx=10, pady=(0, 10))
        ctk.CTkLabel(
            self.frame_obs_guide,
            text="📺 Cómo agregar en OBS:",
            font=ctk.CTkFont(weight="bold", size=13),
            text_color="#64B5F6",
        ).pack(anchor="w", padx=14, pady=(10, 4))
        guide_steps = (
            "1. En OBS, agrega una fuente: «Navegador» (Browser)\n"
            "2. Marca «Archivo local» y selecciona:\n"
            f"   {os.path.abspath('subtitulos_obs.html')}\n"
            "3. Ancho: 1920 | Alto: 200 (o mas segun estilo)\n"
            "4. En la URL, agrega el puerto si no es 8765:\n"
            "   file:///.../subtitulos_obs.html?port=8765\n"
            "5. Listo! Los subtitulos apareceran en vivo."
        )
        ctk.CTkLabel(
            self.frame_obs_guide,
            text=guide_steps,
            wraplength=250,
            justify="left",
            font=ctk.CTkFont(size=11),
            text_color="#B0BEC5",
        ).pack(anchor="w", padx=14, pady=(0, 10))

        # OBS backlog policy (basic)
        ctk.CTkLabel(tab_subtitulos, text="Atraso en OBS:").pack(anchor="w", padx=10)
        current_backlog_policy = BACKLOG_POLICY_BY_VALUE.get(self.config_data.get("subtitle_backlog_policy", "auto"), "Auto (recomendado)")
        self.var_backlog_policy = ctk.StringVar(value=current_backlog_policy)
        self.opt_backlog_policy = ctk.CTkOptionMenu(
            tab_subtitulos,
            values=list(BACKLOG_POLICY_LABELS.keys()),
            variable=self.var_backlog_policy,
            command=self.on_setting_change,
        )
        self.opt_backlog_policy.pack(fill="x", padx=10, pady=(0, 15))

        # Progressive disclosure: advanced subtitle settings
        self._subtitle_advanced_visible = False
        self.frame_subtitle_advanced = ctk.CTkFrame(tab_subtitulos, fg_color="transparent")
        # Hidden by default — shown via grid when toggle is clicked

        self.btn_toggle_subtitle_advanced = ctk.CTkButton(
            tab_subtitulos,
            text="▼ Configuración avanzada",
            fg_color="transparent",
            border_width=1,
            command=self._toggle_advanced_subtitles,
        )
        self.btn_toggle_subtitle_advanced.pack(fill="x", padx=10, pady=(0, 8))

        ctk.CTkLabel(self.frame_subtitle_advanced, text=f"Max atraso live: {self.config_data['subtitle_max_live_delay_sec']}s").pack(anchor="w", padx=10)
        self.lbl_max_live_delay = ctk.CTkLabel(self.frame_subtitle_advanced, text=f"Max atraso live: {self.config_data['subtitle_max_live_delay_sec']}s")
        self.lbl_max_live_delay.pack(anchor="w", padx=10)
        self.slider_max_live_delay = ctk.CTkSlider(self.frame_subtitle_advanced, from_=1.0, to=120.0, command=self.on_setting_change)
        self.slider_max_live_delay.set(self.config_data["subtitle_max_live_delay_sec"])
        self.slider_max_live_delay.pack(fill="x", padx=10, pady=(0, 8))

        self.lbl_catchup_interval = ctk.CTkLabel(self.frame_subtitle_advanced, text=f"Pacing catch-up: {self.config_data['subtitle_catchup_interval_sec']}s")
        self.lbl_catchup_interval.pack(anchor="w", padx=10)
        self.slider_catchup_interval = ctk.CTkSlider(self.frame_subtitle_advanced, from_=0.0, to=10.0, command=self.on_setting_change)
        self.slider_catchup_interval.set(self.config_data["subtitle_catchup_interval_sec"])
        self.slider_catchup_interval.pack(fill="x", padx=10, pady=(0, 15))

        # Blacklist
        ctk.CTkLabel(tab_advanced, text="Filtro Anti-Alucinaciones:").pack(anchor="w", padx=10)
        self.text_blacklist = ctk.CTkTextbox(tab_advanced, height=120)
        self.text_blacklist.insert("0.0", self.config_data["blacklist"])
        self.text_blacklist.pack(fill="x", padx=10, pady=(0, 15))
        # <FocusOut> guarda solo cuando sales del cuadro de texto, ahorrando RAM/Disco
        self.text_blacklist.bind("<FocusOut>", lambda e: self.on_setting_change()) 

        # Botón Principal — en un frame fijo debajo del scroll
        frame_bottom = ctk.CTkFrame(self.screen_main, fg_color="transparent", height=80)
        frame_bottom.grid(row=1, column=0, sticky="ew")

        self.btn_apply = ctk.CTkButton(frame_bottom, text="APLICAR CAMBIOS", height=34, fg_color="#7A4B00", hover_color="#9A6100", command=self.apply_pending_settings)
        self.btn_apply.pack(fill="x", padx=20, pady=(10, 4))
        self.btn_discard = ctk.CTkButton(frame_bottom, text="Descartar cambios", height=28, fg_color="transparent", border_width=1, command=self.discard_pending_settings)
        self.btn_discard.pack(fill="x", padx=20, pady=(0, 6))
        self.btn_power = ctk.CTkButton(frame_bottom, text="INICIAR SISTEMA", height=46, font=ctk.CTkFont(size=16, weight="bold"), command=self.toggle_system)
        self.btn_power.pack(fill="x", padx=20, pady=(0, 10))

        # Panel Derecho (estado principal, preview y debug avanzado)
        frame_der = ctk.CTkFrame(self.screen_main)
        frame_der.grid(row=0, column=1, rowspan=2, sticky="nsew", padx=10, pady=10)
        frame_der.grid_columnconfigure(0, weight=1)
        frame_der.grid_rowconfigure(3, weight=1)

        ctk.CTkLabel(frame_der, text="Panel en vivo", font=ctk.CTkFont(size=22, weight="bold")).grid(row=0, column=0, sticky="w", padx=14, pady=(12, 4))

        frame_status = ctk.CTkFrame(frame_der, fg_color="transparent")
        frame_status.grid(row=1, column=0, sticky="ew", padx=10, pady=(0, 8))
        for i in range(6):
            frame_status.grid_columnconfigure(i, weight=1)

        for idx, key in enumerate(["audio", "vad", "asr", "ws", "obs", "session"]):
            pill = ctk.CTkLabel(
                frame_status,
                text="",
                height=34,
                corner_radius=16,
                fg_color="#263238",
                text_color="#DDE7EA",
                font=ctk.CTkFont(size=12, weight="bold"),
            )
            pill.grid(row=0, column=idx, sticky="ew", padx=3, pady=4)
            self.status_labels[key] = pill

        frame_privacy = ctk.CTkFrame(frame_der, fg_color="#1f2a2d")
        frame_privacy.grid(row=2, column=0, sticky="ew", padx=10, pady=(0, 8))
        frame_privacy.grid_columnconfigure(0, weight=1)
        self.lbl_privacy = ctk.CTkLabel(
            frame_privacy,
            text="Privacidad: ASR local. Los subtítulos se guardan en disco y se emiten por WebSocket local.",
            justify="left",
            wraplength=760,
            text_color="#D0D7DA",
        )
        self.lbl_privacy.grid(row=0, column=0, sticky="ew", padx=12, pady=(10, 2))
        self.lbl_session = ctk.CTkLabel(
            frame_privacy,
            text=f"Carpeta de salida: {self.config_data['output_dir']}",
            justify="left",
            wraplength=760,
            text_color="#AEB8BC",
        )
        self.lbl_session.grid(row=1, column=0, sticky="ew", padx=12, pady=(0, 10))

        frame_preview = ctk.CTkFrame(frame_der)
        frame_preview.grid(row=3, column=0, sticky="nsew", padx=10, pady=(0, 8))
        frame_preview.grid_columnconfigure(0, weight=1)
        frame_preview.grid_rowconfigure(1, weight=1)
        ctk.CTkLabel(frame_preview, text="Ultimo subtitulo enviado", font=ctk.CTkFont(size=16, weight="bold")).grid(row=0, column=0, sticky="w", padx=12, pady=(10, 4))
        self.preview_text = ctk.CTkTextbox(frame_preview, height=170, font=ctk.CTkFont(size=22, weight="bold"), wrap="word")
        self.preview_text.grid(row=1, column=0, sticky="nsew", padx=12, pady=(0, 12))
        self.preview_text.insert("0.0", "Sin transcripciones todavia.")
        self.preview_text.configure(state="disabled")

        frame_advanced_toggle = ctk.CTkFrame(frame_der, fg_color="transparent")
        frame_advanced_toggle.grid(row=4, column=0, sticky="ew", padx=10, pady=(0, 4))
        self.var_advanced = ctk.BooleanVar(value=False)
        self.switch_advanced = ctk.CTkSwitch(frame_advanced_toggle, text="Mostrar logs y diagnostico avanzado", variable=self.var_advanced, command=self.toggle_advanced_logs)
        self.switch_advanced.pack(anchor="w", padx=4, pady=4)

        self.frame_logs = ctk.CTkFrame(frame_der)
        self.frame_logs.grid_columnconfigure(0, weight=1)
        self.frame_logs.grid_rowconfigure(1, weight=1)
        ctk.CTkLabel(self.frame_logs, text="Logs tecnicos (ultimas 300 lineas)", font=ctk.CTkFont(weight="bold")).grid(row=0, column=0, sticky="w", padx=8, pady=(8, 2))
        
        self.consola = ctk.CTkTextbox(self.frame_logs, state="disabled", font=ctk.CTkFont(family="Consolas", size=13), height=180)
        self.consola.grid(row=1, column=0, sticky="nsew", padx=8, pady=(0, 8))
        
        # Validar el slider al inicio
        self._ui_ready = True
        self.refresh_profile_status()
        self.update_ui_state()
        self.set_status("audio", "Audio: listo", "idle")
        self.set_status("vad", "VAD: inactivo", "idle")
        self.set_status("asr", "ASR: detenido", "idle")
        self.set_status("ws", "WS: detenido", "idle")
        self.set_status("obs", "OBS: 0 clientes", "idle")
        self.set_status("session", "Sesion: sin iniciar", "idle")

    # --- LÓGICA DE DISPOSITIVOS DE AUDIO ---
    def _refresh_device_list(self):
        """Reescanea los dispositivos de audio del sistema."""
        try:
            self._audio_devices = list_audio_devices()
        except Exception:
            self._audio_devices = []
        
        self._device_display_list = ["🔄 Por defecto del sistema"]
        for dev in self._audio_devices:
            self._device_display_list.append(dev["display"])

    def _on_refresh_devices(self):
        """Callback del botón de refresh de dispositivos."""
        self._refresh_device_list()
        self.opt_device.configure(values=self._device_display_list)
        self.print_log(f"[Sistema] 🔍 {len(self._audio_devices)} dispositivos de audio detectados.")

    def _read_ui_config(self):
        draft = copy.deepcopy(self.config_data)
        draft["audio_device"] = copy.deepcopy(self.draft_config.get("audio_device", self.config_data.get("audio_device")))
        draft["silence_timeout"] = round(self.slider_silence.get(), 1)
        draft["max_chunk_duration"] = round(self.slider_max_dur.get(), 1)
        draft["subtitle_max_live_delay_sec"] = round(self.slider_max_live_delay.get(), 1)
        draft["subtitle_catchup_interval_sec"] = round(self.slider_catchup_interval.get(), 1)
        draft["device"] = self.var_hw.get()
        draft["model_size"] = self.var_model.get()
        draft["cpu_threads"] = int(self.slider_threads.get())
        draft["continuous_session"] = self.var_session.get()
        draft["blacklist"] = self.text_blacklist.get("0.0", "end").strip()
        draft["subtitle_style"] = self.var_style.get()
        draft["subtitle_backlog_policy"] = BACKLOG_POLICY_LABELS.get(self.var_backlog_policy.get(), "auto")
        draft["obs_enabled"] = self.var_obs_enabled.get()
        draft["whisper_context_prompt"] = self.text_whisper_prompt.get("0.0", "end").strip()
        return draft

    def _load_ui_from_config(self, config):
        self._ui_ready = False
        self.draft_config = copy.deepcopy(config)
        current_device_display = self._device_display_list[0]
        saved_device = config.get("audio_device")
        if saved_device and isinstance(saved_device, dict):
            saved_name = saved_device.get("display", "")
            if saved_name in self._device_display_list:
                current_device_display = saved_name
        self.var_device.set(current_device_display)
        self.var_hw.set(config["device"])
        self.slider_threads.set(config["cpu_threads"])
        self.var_model.set(next((m for m in self.opt_model.cget("values") if m.startswith(config["model_size"].split()[0])), "small (Balance CPU)"))
        self.slider_silence.set(config["silence_timeout"])
        self.slider_max_dur.set(config["max_chunk_duration"])
        self.var_session.set(config["continuous_session"])
        self.var_style.set(config.get("subtitle_style", "default"))
        self.var_backlog_policy.set(BACKLOG_POLICY_BY_VALUE.get(config.get("subtitle_backlog_policy", "auto"), "Auto (recomendado)"))
        self.slider_max_live_delay.set(config["subtitle_max_live_delay_sec"])
        self.slider_catchup_interval.set(config["subtitle_catchup_interval_sec"])
        self.text_blacklist.delete("0.0", "end")
        self.text_blacklist.insert("0.0", config.get("blacklist", ""))
        self.var_obs_enabled.set(config.get("obs_enabled", True))
        self.text_whisper_prompt.delete("0.0", "end")
        self.text_whisper_prompt.insert("0.0", config.get("whisper_context_prompt", ""))
        self._ui_ready = True
        self.on_setting_change()

    def _profile_id_for_current_values(self, config):
        for profile_id, profile in PROFILE_PRESETS.items():
            if all(config.get(key) == value for key, value in profile["values"].items()):
                return profile_id
        return "custom"

    def refresh_profile_status(self):
        if not self._ui_ready:
            return
        draft = self._read_ui_config()
        profile_id = self._profile_id_for_current_values(draft)
        if profile_id != "custom":
            self.var_profile.set(PROFILE_PRESETS[profile_id]["label"])
            self.lbl_profile_desc.configure(text=PROFILE_PRESETS[profile_id]["description"])
        elif self.var_profile.get() != "Personalizado":
            self.lbl_profile_desc.configure(text="Perfil modificado: se aplicará como Personalizado.")
        else:
            self.lbl_profile_desc.configure(text="Ajustes manuales avanzados.")

        has_pending = any(draft.get(key) != self.config_data.get(key) for key in draft.keys())
        if has_pending:
            self.lbl_pending.configure(text="Cambios pendientes: pulsa Aplicar cambios", text_color="#FFC107")
            self.btn_apply.configure(state="normal")
            self.btn_discard.configure(state="normal")
        else:
            self.lbl_pending.configure(text="Configuración activa", text_color="#8BC34A")
            self.btn_apply.configure(state="disabled")
            self.btn_discard.configure(state="disabled")

    def on_profile_select(self, selected):
        if not self._ui_ready or selected == "Personalizado":
            return
        current_draft = self._read_ui_config()
        has_pending = any(current_draft.get(key) != self.config_data.get(key) for key in current_draft.keys())
        if has_pending and not messagebox.askyesno("Cambios pendientes", "Cambiar de perfil descartará los cambios pendientes. ¿Continuar?"):
            self.refresh_profile_status()
            return

        profile_id = PROFILE_LABEL_TO_ID.get(selected)
        if not profile_id:
            return
        draft = copy.deepcopy(self.config_data)
        draft.update(PROFILE_PRESETS[profile_id]["values"])
        draft["selected_profile_id"] = profile_id
        draft["profile_mode"] = "preset"
        self._load_ui_from_config(draft)

    def on_device_change(self, *args):
        """Callback cuando se cambia el dispositivo de audio."""
        selected = self.var_device.get()
        
        if selected == self._device_display_list[0]:
            # "Por defecto del sistema"
            self.draft_config["audio_device"] = None
        else:
            # Buscar el dispositivo por su display name
            device = next((d for d in self._audio_devices if d["display"] == selected), None)
            if device:
                self.draft_config["audio_device"] = device
        self.refresh_profile_status()

    # --- LÓGICA DE CONTROL ---
    def update_ui_state(self):
        """Bloquea o desbloquea elementos de la UI según la selección"""
        if self.var_hw.get() == "cuda":
            self.slider_threads.configure(state="disabled", button_color="gray")
        else:
            self.slider_threads.configure(state="normal", button_color=["#3B8ED0", "#1F6AA5"])

    def on_setting_change(self, *args):
        if not self._ui_ready:
            return
        self.update_ui_state()

        self.lbl_silence.configure(text=f"Detección de Silencio: {self.slider_silence.get():.1f}s")
        self.lbl_max_dur.configure(text=f"Duración máxima de frase: {self.slider_max_dur.get():.1f}s")
        self.lbl_max_live_delay.configure(text=f"Max atraso live: {self.slider_max_live_delay.get():.1f}s")
        self.lbl_catchup_interval.configure(text=f"Pacing catch-up: {self.slider_catchup_interval.get():.1f}s")
        self.refresh_profile_status()

    def _on_style_change(self, *args):
        """Handle style change: update preview and trigger settings change."""
        self._update_preview()
        self.on_setting_change()

    def _toggle_advanced_subtitles(self):
        """Toggle visibility of advanced subtitle settings (progressive disclosure)."""
        self._subtitle_advanced_visible = not self._subtitle_advanced_visible
        if self._subtitle_advanced_visible:
            self.frame_subtitle_advanced.pack(fill="x", padx=10, pady=(0, 8))
            self.btn_toggle_subtitle_advanced.configure(text="▲ Ocultar configuración avanzada")
        else:
            self.frame_subtitle_advanced.pack_forget()
            self.btn_toggle_subtitle_advanced.configure(text="▼ Configuración avanzada")

    def _on_obs_toggle(self, *args):
        """Handle OBS enable/disable toggle change."""
        self.on_setting_change()

    def _pending_restart_flags(self, draft):
        needs_asr_restart = any(self.config_data.get(key) != draft.get(key) for key in ["device", "model_size", "cpu_threads"])
        needs_audio_restart = any(self.config_data.get(key) != draft.get(key) for key in ["audio_device", "silence_timeout", "max_chunk_duration"])
        return needs_asr_restart, needs_audio_restart

    def _validate_draft_config(self, draft):
        if draft.get("device") == "cuda":
            if not torch.cuda.is_available():
                raise RuntimeError("CUDA no esta disponible. Cambia a CPU o libera/configura la GPU antes de aplicar este perfil.")
            # Run a small CUDA test to verify the GPU is actually usable
            try:
                torch.zeros(1).cuda()
            except Exception as e:
                raise RuntimeError(f"CUDA no responde: {e}. Cambia a CPU antes de aplicar.")

        audio_device = draft.get("audio_device")
        if audio_device is not None:
            selected_display = audio_device.get("display") if isinstance(audio_device, dict) else None
            if selected_display not in self._device_display_list:
                raise RuntimeError("El dispositivo de audio seleccionado ya no está disponible. Refresca la lista o usa el dispositivo por defecto.")

    def apply_pending_settings(self):
        if self._applying_settings:
            return
        draft = self._read_ui_config()
        profile_id = self._profile_id_for_current_values(draft)
        if profile_id == "custom" and self.var_profile.get() != "Personalizado":
            response = messagebox.askyesnocancel(
                "Perfil modificado",
                "Modificaste un perfil integrado.\n\nSí: aplicar como Personalizado.\nNo: descartar cambios.\nCancelar: seguir editando.",
            )
            if response is None:
                return
            if response is False:
                self.discard_pending_settings()
                return
            draft["selected_profile_id"] = "custom"
            draft["profile_mode"] = "custom"
        else:
            draft["selected_profile_id"] = profile_id
            draft["profile_mode"] = "preset" if profile_id != "custom" else "custom"

        needs_asr_restart, needs_audio_restart = self._pending_restart_flags(draft)
        if self.is_running and (needs_asr_restart or needs_audio_restart):
            if not messagebox.askyesno(
                "Aplicar cambios en vivo",
                "Algunos cambios requieren hot-swap del motor. Puede haber un corte breve y perderse la frase actual. ¿Aplicar ahora?",
            ):
                return

        self._applying_settings = True
        self.btn_apply.configure(state="disabled")
        try:
            # Validate output_dir writability before applying
            output_dir = draft.get("output_dir", "")
            is_valid, error_msg = _validate_output_dir(output_dir)
            if not is_valid:
                raise ValueError(error_msg)

            previous_config = copy.deepcopy(self.config_data)
            self._validate_draft_config(draft)
            self.config_data = copy.deepcopy(draft)
            for k, v in self.config_data.items():
                self.shared_config[k] = v

            if self.is_running and (needs_asr_restart or needs_audio_restart):
                self.print_log("\n[Sistema] Aplicando cambios con hot-swap en vivo...")
                self.set_status("asr", "ASR: aplicando cambios", "warn")
                self.set_status("vad", "VAD: aplicando cambios", "warn")
                if not self.shared_config["continuous_session"]:
                    timestamp = datetime.datetime.now().strftime("%Y-%m-%d_%H%M%S")
                    self.current_session_dir = os.path.join(self.shared_config["output_dir"], f"session_{timestamp}")
                    self.update_session_label()
                if not self.hot_swap_engine():
                    raise RuntimeError("El hot-swap no pudo arrancar los procesos de audio/ASR.")

            save_config(self.config_data)
            self.draft_config = copy.deepcopy(self.config_data)
            self.var_profile.set(PROFILE_PRESETS.get(self.config_data.get("selected_profile_id"), {}).get("label", "Personalizado"))
            self.refresh_profile_status()
            self.print_log("[Sistema] Configuración aplicada y guardada.")
        except Exception as e:
            self.config_data = previous_config
            for k, v in self.config_data.items():
                self.shared_config[k] = v
            if self.is_running and (needs_asr_restart or needs_audio_restart):
                self.hot_swap_engine()
            messagebox.showerror("Error al aplicar", f"No se pudieron aplicar los cambios:\n{e}")
            self.print_log(f"[Sistema] Error al aplicar cambios: {e}")
        finally:
            self._applying_settings = False
            self.refresh_profile_status()

    def discard_pending_settings(self):
        self.draft_config = copy.deepcopy(self.config_data)
        self._load_ui_from_config(self.config_data)
        self.var_profile.set(PROFILE_PRESETS.get(self.config_data.get("selected_profile_id"), {}).get("label", "Personalizado"))
        self.refresh_profile_status()

    def print_log(self, msg):
        if not isinstance(msg, str):
            msg = str(msg)
        self._log_lines.append(msg)
        if len(self._log_lines) > LOG_MAX_LINES:
            self._log_lines = self._log_lines[-LOG_MAX_LINES:]
        self.consola.configure(state="normal")
        self.consola.delete("0.0", "end")
        self.consola.insert("end", "\n".join(self._log_lines) + "\n")
        self.consola.see("end")
        self.consola.configure(state="disabled")

    def set_status(self, key, text, state="idle"):
        colors = {
            "idle": "#263238",
            "ok": "#1B5E20",
            "active": "#0D47A1",
            "warn": "#7A4B00",
            "error": "#7F1D1D",
        }
        label = self.status_labels.get(key)
        if label:
            label.configure(text=text, fg_color=colors.get(state, colors["idle"]))

    def set_preview(self, text):
        clean_text = " ".join(str(text).split())
        if len(clean_text) > PREVIEW_MAX_CHARS:
            clean_text = clean_text[:PREVIEW_MAX_CHARS].rstrip() + "..."
        self.preview_text.configure(state="normal")
        self.preview_text.delete("0.0", "end")
        self.preview_text.insert("0.0", clean_text or "Sin transcripciones todavia.")
        self.preview_text.configure(state="disabled")

    def _update_preview(self):
        """Update static preview panel based on selected style from PRESET_STYLES."""
        style_name = self.var_style.get()
        style = PRESET_STYLES.get(style_name, PRESET_STYLES["default"])
        if hasattr(self, "frame_preview"):
            self.frame_preview.configure(fg_color=style["fg_color"])
        if hasattr(self, "lbl_preview_sample"):
            self.lbl_preview_sample.configure(
                text=PREVIEW_SAMPLE_TEXT,
                text_color=style["text_color"],
                font=ctk.CTkFont(size=18, weight=style["font_weight"], family=style["font_family"]),
            )

    def update_session_label(self):
        if self.current_session_dir:
            text = f"Sesion activa: {self.current_session_dir}"
            self.set_status("session", "Sesion: guardando", "ok")
        else:
            text = f"Carpeta de salida: {self.config_data['output_dir']}"
            self.set_status("session", "Sesion: sin iniciar", "idle")
        self.lbl_session.configure(text=text)

    def toggle_advanced_logs(self):
        self._advanced_visible = self.var_advanced.get()
        if self._advanced_visible:
            self.frame_logs.grid(row=5, column=0, sticky="nsew", padx=10, pady=(0, 10))
        else:
            self.frame_logs.grid_forget()

    def handle_event(self, event):
        event_type = event.get("type")
        if event_type == "status":
            self.set_status(event.get("key"), event.get("text", ""), event.get("state", "idle"))
        elif event_type == "transcript":
            latency = event.get("latency")
            total_delay = event.get("total_delay")
            obs_emitted = event.get("obs_emitted", True)
            if obs_emitted:
                self.set_preview(event.get("text", ""))
                if latency is not None and total_delay is not None:
                    self.print_log(f"[IA] Subtitulo enviado a OBS ({latency:.2f}s ASR, {total_delay:.1f}s total). Texto oculto en logs por privacidad.")
                elif latency is not None:
                    self.print_log(f"[IA] Subtitulo enviado a OBS ({latency:.2f}s). Texto oculto en logs por privacidad.")
                else:
                    self.print_log("[IA] Subtitulo enviado a OBS. Texto oculto en logs por privacidad.")
            else:
                reason = event.get("reason", "policy")
                if total_delay is not None:
                    self.print_log(f"[IA] Transcripcion guardada, no enviada a OBS ({reason}, {total_delay:.1f}s total).")
                else:
                    self.print_log(f"[IA] Transcripcion guardada, no enviada a OBS ({reason}).")
        elif event_type == "log":
            self.print_log(event.get("message", ""))

    def process_logs(self):
        while True:
            try:
                msg = self.log_queue.get_nowait()
                if isinstance(msg, dict):
                    self.handle_event(msg)
                else:
                    self.print_log(msg)
            except queue.Empty:
                break
        self.after(100, self.process_logs)

    def _stop_process(self, proc, name="proceso", timeout=3):
        """Apagado limpio de un proceso: espera con timeout, luego mata."""
        if proc is None or not proc.is_alive():
            return
        proc.join(timeout=timeout)
        if proc.is_alive():
            proc.terminate()
            proc.join(timeout=2)

    def _drain_queue(self, q):
        """Vacía una cola para evitar que bloquee procesos al cerrarse."""
        try:
            while not q.empty():
                q.get_nowait()
        except Exception:
            pass

    def hot_swap_engine(self):
        # --- DEFENSA 4: Anti-Corrupción de Pipes ---
        # Si matamos la IA mientras lee, rompemos la tubería. 
        # La solución es reiniciar a ambos (IA y Productor) con una NUEVA cola.
        
        # Enviar señal de apagado limpio
        if self.p_audio and self.p_audio.is_alive():
            self.p_audio.terminate()
        if self.p_ia and self.p_ia.is_alive():
            try:
                self.audio_queue.put_nowait(None)  # Señal de fin
            except Exception:
                pass
            self._stop_process(self.p_ia, "IA")
        
        # Esperar y limpiar
        self._stop_process(self.p_audio, "Productor")
        self._drain_queue(self.audio_queue)
        
        self.audio_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)  # Tubería 100% nueva y limpia

        self.p_audio = mp.Process(target=audio_producer, args=(self.audio_queue, self.shared_config, self.log_queue), daemon=True)
        self.p_ia = mp.Process(target=asr_consumer, args=(self.audio_queue, self.text_queue, self.log_queue, self.shared_config, self.current_session_dir), daemon=True)
        self.p_audio.start()
        self.p_ia.start()
        self.after(3000, self.refresh_profile_status)
        return self.p_audio.is_alive() and self.p_ia.is_alive()

    def toggle_system(self):
        if not self.is_running and self._ui_ready:
            draft = self._read_ui_config()
            has_pending = any(draft.get(key) != self.config_data.get(key) for key in draft.keys())
            if has_pending:
                response = messagebox.askyesnocancel("Cambios pendientes", "Hay cambios pendientes antes de iniciar.\n\nSí: aplicar e iniciar.\nNo: iniciar sin aplicar.\nCancelar: volver.")
                if response is None:
                    return
                if response is True:
                    self.apply_pending_settings()
                    draft = self._read_ui_config()
                    if any(draft.get(key) != self.config_data.get(key) for key in draft.keys()):
                        return
        if not self.is_running:
            self.is_running = True
            self.btn_power.configure(text="DETENER SISTEMA", fg_color="darkred", hover_color="red")
            
            # Limpiar consola (habilitar → borrar → deshabilitar)
            self._log_lines = []
            self.consola.configure(state="normal")
            self.consola.delete("0.0", "end")
            self.consola.configure(state="disabled")
            
            self.print_log("[Sistema] Iniciando servicios base...")
            self.set_status("audio", "Audio: iniciando", "active")
            self.set_status("vad", "VAD: cargando", "active")
            self.set_status("asr", "ASR: cargando", "active")
            self.set_status("ws", "WS: iniciando", "active")
            self.set_status("obs", "OBS: 0 clientes", "idle")
            
            # Generar carpeta principal de la sesión
            timestamp = datetime.datetime.now().strftime("%Y-%m-%d_%H%M%S")
            self.current_session_dir = os.path.join(self.shared_config["output_dir"], f"session_{timestamp}")
            self.update_session_label()
            
            # Recrear cola de texto para evitar pipes corruptos entre sesiones
            self.text_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)
            
            ws_port = self.shared_config.get("ws_port", 8765)
            self.p_ws = mp.Process(target=run_ws_server, args=(self.text_queue, self.log_queue, ws_port), daemon=True)
            self.p_ws.start()
            
            self.hot_swap_engine()
        else:
            self.is_running = False
            self.btn_power.configure(text="INICIAR SISTEMA", fg_color=["#3B8ED0", "#1F6AA5"], hover_color=["#36719F", "#144870"])
            self.print_log("[Sistema] Apagando todos los servicios...")
            self.set_status("audio", "Audio: detenido", "idle")
            self.set_status("vad", "VAD: inactivo", "idle")
            self.set_status("asr", "ASR: detenido", "idle")
            self.set_status("ws", "WS: detenido", "idle")
            self.set_status("obs", "OBS: 0 clientes", "idle")
            
            # Apagado limpio con señal → join → terminate
            if self.p_ia and self.p_ia.is_alive():
                try:
                    self.audio_queue.put_nowait(None)
                except Exception:
                    pass
            
            self._stop_process(self.p_audio, "Productor")
            self._stop_process(self.p_ia, "IA")
            
            # Señal de apagado limpio al servidor WebSocket antes de matar el proceso
            try:
                self.text_queue.put_nowait(None)
            except Exception:
                pass
            
            self._stop_process(self.p_ws, "WebSocket")
            
            self._drain_queue(self.audio_queue)
            self._drain_queue(self.text_queue)
            self.current_session_dir = None
            self.update_session_label()

    def on_closing(self):
        """Maneja el evento de cerrar la ventana (X) para evitar procesos zombies"""
        if self._ui_ready:
            draft = self._read_ui_config()
            has_pending = any(draft.get(key) != self.config_data.get(key) for key in draft.keys())
            if has_pending:
                response = messagebox.askyesnocancel("Cambios pendientes", "Hay cambios pendientes.\n\nSí: aplicar y cerrar.\nNo: descartar y cerrar.\nCancelar: volver.")
                if response is None:
                    return
                if response is True:
                    self.apply_pending_settings()
                    draft = self._read_ui_config()
                    if any(draft.get(key) != self.config_data.get(key) for key in draft.keys()):
                        return
        self.is_running = False
        
        # Señal de apagado limpio al servidor WebSocket
        try:
            self.text_queue.put_nowait(None)
        except Exception:
            pass
        
        # Apagado limpio de todos los procesos
        for proc in [self.p_audio, self.p_ia, self.p_ws]:
            self._stop_process(proc, timeout=2)
        
        # Drenar colas para desbloquear cualquier proceso
        for q in [self.audio_queue, self.text_queue, self.log_queue]:
            self._drain_queue(q)
        
        self.destroy()


if __name__ == '__main__':
    mp.freeze_support()
    app = LiveASRApp()
    app.mainloop()
