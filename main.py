# SPDX-License-Identifier: MIT
import os
import queue
import json
import datetime
import copy
import multiprocessing as mp
import customtkinter as ctk
import torch
import webbrowser
from PIL import Image, ImageTk
from tkinter import filedialog, messagebox

torch_lib_path = os.path.join(os.path.dirname(torch.__file__), "lib")
os.environ["PATH"] = f"{torch_lib_path};{os.environ.get('PATH', '')}"

from utils.config import load_config, save_config
from utils.i18n import t, set_language, autodetect_language, get_language
from core.engine import asr_consumer
from core.audio import audio_producer, list_audio_devices
from core.network import run_ws_server
from core.diagnostics import build_diagnostics_report, normalize_export_dir

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


def _safe_queue_size(queue_obj):
    if queue_obj is None:
        return None
    try:
        return queue_obj.qsize()
    except Exception:
        return None


def build_app_runtime_summary(app_state: dict) -> dict:
    """Create a bounded local runtime snapshot from app-visible state."""
    processes = app_state.get("processes", {})
    queues = app_state.get("queues", {})
    return {
        "is_running": bool(app_state.get("is_running")),
        "session_dir": app_state.get("session_dir"),
        "statuses": dict(app_state.get("statuses", {})),
        "processes": {
            name: bool(proc.is_alive()) if proc is not None else False
            for name, proc in processes.items()
        },
        "queues": {name: value for name, value in queues.items()},
    }


def export_local_diagnostics_report(report: dict, export_dir: str) -> str:
    """Persist a diagnostics report locally as JSON."""
    normalized_dir = normalize_export_dir(export_dir)
    if not normalized_dir:
        raise ValueError("Diagnostics export directory is not configured.")
    if "generated_at" not in report:
        report = build_diagnostics_report(
            runtime_health=report.get("runtime"),
            test_health=report.get("test_health"),
        )
    os.makedirs(normalized_dir, exist_ok=True)
    timestamp = datetime.datetime.now().strftime("%Y-%m-%d_%H%M%S")
    file_path = os.path.join(normalized_dir, f"liveaudio_diagnostics_{timestamp}.json")
    with open(file_path, "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2, ensure_ascii=False)
    return file_path


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
        
        # --- ICONO PERSONALIZADO (Chihuahua Kawaii y Taskbar Fix) ---
        try:
            # 1. Resolver ID de aplicación para forzar icono desacoplado en la barra de tareas de Windows
            import ctypes
            myappid = 'plynte.liveaudio.chihuahua.v1'
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(myappid)
        except Exception:
            pass

        try:
            # 2. Generar el archivo .ico al vuelo desde el .png de forma dinámica si no existe
            png_path = os.path.join(os.path.dirname(__file__), "LiveAudio-Fran.png")
            ico_path = os.path.join(os.path.dirname(__file__), "LiveAudio-Fran.ico")
            
            if os.path.exists(png_path) and not os.path.exists(ico_path):
                img = Image.open(png_path)
                img.save(ico_path, format="ICO", sizes=[(16, 16), (32, 32), (48, 48), (256, 256)])
            
            if os.path.exists(ico_path):
                self.iconbitmap(ico_path)
        except Exception as e:
            print(f"[Icono] No se pudo cargar el icono: {e}")
        
        # --- DEFENSA 1: Manejo del botón 'X' de Windows ---
        self.protocol("WM_DELETE_WINDOW", self.on_closing)
        
        self.config_data = load_config()
        # --- CONFIGURACIÓN DE IDIOMA ---
        lang = self.config_data.get("language")
        if not lang:
            lang = autodetect_language()
        set_language(lang)

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

    def _create_premium_option_menu(self, master, values, variable, command=None, width=140, **kwargs):
        """Crea un CTkOptionMenu con un estilo premium personalizado y coherente con el tema verde."""
        menu = ctk.CTkOptionMenu(
            master,
            values=values,
            variable=variable,
            command=command,
            width=width,
            fg_color="#1a2730",               # Botón fondo oscuro sutil
            button_color="#2d7a4d",           # Flecha con acento verde
            button_hover_color="#3c9e66",     # Hover flecha verde brillante
            dropdown_fg_color="#182226",      # Fondo del menú desplegable
            dropdown_hover_color="#2d7a4d",   # Hover de los items en verde
            dropdown_text_color="#E8E8E8",    # Texto claro premium
            dropdown_font=ctk.CTkFont(family="Segoe UI", size=12),
            font=ctk.CTkFont(family="Segoe UI", size=12),
            dynamic_resizing=True,            # Auto-ancho según contenido para ver nombres largos
            **kwargs
        )
        return menu

    def _on_language_change(self, selected_lang):
        """Callback cuando cambia el idioma."""
        lang_code = "es" if selected_lang in ("Español", "Spanish") else "en"
        set_language(lang_code)
        self.config_data["language"] = lang_code
        save_config(self.config_data)
        self._rebuild_ui()

    def _rebuild_ui(self):
        """Reconstruye la pantalla activa para aplicar el nuevo idioma inmediatamente."""
        self._ui_ready = False
        
        # Detectar qué pantalla está visible y reconstruirla
        if hasattr(self, "screen_welcome") and self.screen_welcome.winfo_ismapped():
            # Limpiar bienvenida
            for widget in self.screen_welcome.winfo_children():
                widget.destroy()
            self.build_welcome_screen()
        elif hasattr(self, "screen_main") and self.screen_main.winfo_ismapped():
            # Limpiar principal
            if hasattr(self, "screen_main") and self.screen_main:
                self.screen_main.destroy()
            self.screen_main = ctk.CTkFrame(self, fg_color="transparent")
            self.build_main_screen()
            self.screen_main.grid(row=0, column=0, sticky="nsew")

    # --- PANTALLA 1: BIENVENIDA ---
    def build_welcome_screen(self):
        # Frame contenedor con estilo premium de tarjeta flotante
        container = ctk.CTkFrame(
            self.screen_welcome, 
            width=780, 
            height=480, 
            corner_radius=16, 
            border_width=1, 
            border_color="#1f2d33", 
            fg_color="#111b1e"
        )
        container.place(relx=0.5, rely=0.5, anchor="center")
        container.grid_propagate(False)
        container.pack_propagate(False)

        # Configurar columnas del contenedor principal
        container.grid_columnconfigure(0, weight=1)  # Columna izquierda (Héroe/Logo/Tips)
        container.grid_columnconfigure(1, minsize=1)  # Línea divisoria
        container.grid_columnconfigure(2, weight=1)  # Columna derecha (Configuración/Acciones)
        container.grid_rowconfigure(0, weight=1)

        # --- PANEL IZQUIERDO (Branding & Info Hero) ---
        left_panel = ctk.CTkFrame(container, fg_color="transparent")
        left_panel.grid(row=0, column=0, sticky="nsew", padx=(25, 15), pady=20)
        
        # Cargar logo de Chihuahua / LiveAudio
        logo_path = os.path.join(os.path.dirname(__file__), "LiveAudio-Fran.png")
        if os.path.exists(logo_path):
            try:
                pil_img = Image.open(logo_path)
                logo_img = ctk.CTkImage(light_image=pil_img, dark_image=pil_img, size=(100, 100))
                lbl_logo = ctk.CTkLabel(left_panel, image=logo_img, text="")
                lbl_logo.pack(pady=(15, 5))
            except Exception as e:
                print(f"[UI] Error al cargar logo en bienvenida: {e}")
                
        ctk.CTkLabel(
            left_panel, 
            text=t("welcome_title"), 
            font=ctk.CTkFont(size=34, weight="bold", family="Outfit"), 
            text_color="#3c9e66"
        ).pack(pady=(0, 2))
        
        lbl_by = ctk.CTkLabel(
            left_panel, 
            text=t("welcome_by"), 
            font=ctk.CTkFont(size=12, weight="bold", family="Outfit"), 
            text_color="#AEB8BC", 
            cursor="hand2"
        )
        lbl_by.pack(pady=(0, 15))
        lbl_by.bind("<Button-1>", lambda e: webbrowser.open_new("https://github.com/plynte-labs/LiveAudio"))

        # Tarjeta de tips estilizada
        tips_card = ctk.CTkFrame(left_panel, fg_color="#0a1214", corner_radius=12, border_width=1, border_color="#18272c")
        tips_card.pack(fill="both", expand=True, padx=5, pady=(0, 10))
        
        tips_header = "💡 Configuración / Tips" if get_language() == "es" else "💡 Configuration / Tips"
        ctk.CTkLabel(
            tips_card, 
            text=tips_header, 
            font=ctk.CTkFont(size=13, weight="bold", family="Segoe UI"), 
            text_color="#3c9e66"
        ).pack(anchor="w", padx=15, pady=(12, 6))

        tips_text = t("tips_text")
        if tips_text.startswith("💡"):
            tips_text = tips_text[1:].strip()
            
        ctk.CTkLabel(
            tips_card, 
            text=tips_text, 
            justify="left", 
            wraplength=310, 
            font=ctk.CTkFont(size=12, family="Segoe UI"), 
            text_color="#AEB8BC"
        ).pack(anchor="w", padx=15, pady=(0, 12))

        # --- DIVISOR CENTRAL ---
        divider = ctk.CTkFrame(container, width=1, fg_color="#1f2d33")
        divider.grid(row=0, column=1, sticky="ns", pady=40)

        # --- PANEL DERECHO (Configuración inicial) ---
        right_panel = ctk.CTkFrame(container, fg_color="transparent")
        right_panel.grid(row=0, column=2, sticky="nsew", padx=(15, 25), pady=20)

        setup_title = "Ajustes de Inicio" if get_language() == "es" else "Initial Setup"
        ctk.CTkLabel(
            right_panel, 
            text=setup_title, 
            font=ctk.CTkFont(size=22, weight="bold", family="Outfit"), 
            text_color="#E8E8E8"
        ).pack(anchor="w", padx=10, pady=(15, 12))

        # Segmento: Carpeta de Sesiones
        folder_frame = ctk.CTkFrame(right_panel, fg_color="#0a1214", corner_radius=10, border_width=1, border_color="#18272c")
        folder_frame.pack(fill="x", padx=10, pady=5)
        
        ctk.CTkLabel(
            folder_frame, 
            text=t("current_sessions_folder"), 
            font=ctk.CTkFont(size=11, weight="bold", family="Segoe UI"), 
            text_color="#AEB8BC"
        ).pack(anchor="w", padx=12, pady=(8, 2))
        
        self.lbl_folder = ctk.CTkLabel(
            folder_frame, 
            text=self.config_data["output_dir"], 
            font=ctk.CTkFont(size=11, family="Consolas"), 
            text_color="gray", 
            wraplength=280, 
            justify="left"
        )
        self.lbl_folder.pack(anchor="w", padx=12, pady=(0, 8))
        
        btn_folder = ctk.CTkButton(
            folder_frame, 
            text=t("change_folder"), 
            height=24, 
            font=ctk.CTkFont(size=11, weight="bold", family="Segoe UI"), 
            fg_color="transparent", 
            border_width=1, 
            command=self.change_folder
        )
        btn_folder.pack(anchor="w", padx=12, pady=(0, 10))

        # Segmento: Diseño del menú
        ctk.CTkLabel(
            right_panel, 
            text=t("settings_layout"), 
            font=ctk.CTkFont(size=11, weight="bold", family="Segoe UI"), 
            text_color="#AEB8BC"
        ).pack(anchor="w", padx=15, pady=(8, 2))
        
        nav_choices = [t("tabs_horizontal"), t("dropdown_menu")]
        current_nav = t("tabs_horizontal") if self.config_data.get("settings_navigation_mode", "tabs") == "tabs" else t("dropdown_menu")
        
        self.var_welcome_nav_mode = ctk.StringVar(value=current_nav)
        self.opt_welcome_nav_mode = self._create_premium_option_menu(
            right_panel,
            values=nav_choices,
            variable=self.var_welcome_nav_mode,
            command=self._on_welcome_nav_mode_change,
            width=290
        )
        self.opt_welcome_nav_mode.pack(fill="x", padx=10, pady=(0, 8))

        # Segmento: Selector de Idioma
        ctk.CTkLabel(
            right_panel, 
            text=t("welcome_lang_label"), 
            font=ctk.CTkFont(size=11, weight="bold", family="Segoe UI"), 
            text_color="#AEB8BC"
        ).pack(anchor="w", padx=15, pady=(8, 2))
        
        lang_values = ["Español", "English"]
        current_lang_display = "Español" if get_language() == "es" else "English"
        
        self.var_welcome_lang = ctk.StringVar(value=current_lang_display)
        self.opt_welcome_lang = self._create_premium_option_menu(
            right_panel,
            values=lang_values,
            variable=self.var_welcome_lang,
            command=self._on_language_change,
            width=290
        )
        self.opt_welcome_lang.pack(fill="x", padx=10, pady=(0, 20))

        # Botón de Continuar
        btn_continue = ctk.CTkButton(
            right_panel, 
            text=t("continue").upper(), 
            height=44, 
            font=ctk.CTkFont(size=14, weight="bold", family="Segoe UI"), 
            fg_color="#2d7a4d", 
            hover_color="#3c9e66", 
            command=self.go_to_main
        )
        btn_continue.pack(fill="x", padx=10, pady=(5, 10))

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
        # Destruir y reconstruir la pantalla principal para aplicar el diseño elegido inmediatamente
        if hasattr(self, "screen_main") and self.screen_main:
            self.screen_main.destroy()
        
        self.screen_main = ctk.CTkFrame(self, fg_color="transparent")
        self._ui_ready = False
        self.build_main_screen()
        
        self.screen_welcome.grid_forget() # Ocultar bienvenida
        self.screen_main.grid(row=0, column=0, sticky="nsew") # Mostrar principal

    # --- PANTALLA 2: MOTOR ASR ---
    def build_main_screen(self):
        self.screen_main.grid_columnconfigure(1, weight=1)
        self.screen_main.grid_rowconfigure(0, weight=1)

        # Panel Izquierdo (Ajustes) — con scroll para manejar muchos controles
        frame_izq = ctk.CTkScrollableFrame(self.screen_main, width=320, corner_radius=0)
        frame_izq.grid(row=0, column=0, sticky="nsew")
        
        ctk.CTkLabel(frame_izq, text=t("settings"), font=ctk.CTkFont(size=20, weight="bold")).pack(pady=20, padx=20)

        # Cargar el modo de navegación configurado ("tabs" o "dropdown")
        nav_mode = self.config_data.get("settings_navigation_mode", "tabs")
        
        cat_audio = t("audio_vad")
        cat_model = t("performance")
        cat_subtitles = t("subtitles")
        cat_advanced = t("advanced")

        if nav_mode == "tabs":
            self.settings_tabview = ctk.CTkTabview(frame_izq)
            self.settings_tabview.pack(fill="both", expand=True, padx=10, pady=(0, 10))
            
            tab_audio = self.settings_tabview.add(cat_audio)
            tab_model = self.settings_tabview.add(cat_model)
            tab_subtitulos = self.settings_tabview.add(cat_subtitles)
            tab_advanced = self.settings_tabview.add(cat_advanced)
        else:
            # Título indicador para el dropdown
            ctk.CTkLabel(frame_izq, text=t("settings_section"), font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=15, pady=(0, 5))
            
            # Selector de categorías por Dropdown
            self.var_nav_category = ctk.StringVar(value=cat_audio)
            self.opt_nav_category = self._create_premium_option_menu(
                frame_izq,
                values=[cat_audio, cat_model, cat_subtitles, cat_advanced],
                variable=self.var_nav_category,
                command=self._on_nav_category_change
            )
            self.opt_nav_category.pack(fill="x", padx=10, pady=(0, 15))
            
            # Contenedor principal de ajustes
            self.settings_container = ctk.CTkFrame(frame_izq, fg_color="transparent")
            self.settings_container.pack(fill="both", expand=True)
            
            # Sub-frames equivalentes a las pestañas
            tab_audio = ctk.CTkFrame(self.settings_container, fg_color="transparent")
            tab_model = ctk.CTkFrame(self.settings_container, fg_color="transparent")
            tab_subtitulos = ctk.CTkFrame(self.settings_container, fg_color="transparent")
            tab_advanced = ctk.CTkFrame(self.settings_container, fg_color="transparent")
            
            self._nav_frames = {
                cat_audio: tab_audio,
                cat_model: tab_model,
                cat_subtitles: tab_subtitulos,
                cat_advanced: tab_advanced
            }
            
            # Mostrar por defecto solo la primera sección
            tab_audio.pack(fill="both", expand=True, padx=5)

        # === SECCIÓN: Dispositivo de Audio ===
        ctk.CTkLabel(tab_audio, text=t("audio_device_label"), font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        
        # Frame para el selector + botón refresh
        frame_device = ctk.CTkFrame(tab_audio, fg_color="transparent")
        frame_device.pack(fill="x", padx=10, pady=(0, 5))
        frame_device.grid_columnconfigure(0, weight=1)
        
        self._audio_devices = []
        self._device_display_list = [t("system_default")]
        self._refresh_device_list()
        
        # Determinar selección actual
        current_device_display = self._device_display_list[0]
        saved_device = self.config_data.get("audio_device")
        if saved_device and isinstance(saved_device, dict):
            saved_name = saved_device.get("display", "")
            if saved_name in self._device_display_list:
                current_device_display = saved_name
        
        self.var_device = ctk.StringVar(value=current_device_display)
        self.opt_device = self._create_premium_option_menu(
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
        ctk.CTkLabel(tab_model, text=t("hardware"), font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        self.var_hw = ctk.StringVar(value=self.config_data["device"])
        self.opt_hw = ctk.CTkSegmentedButton(tab_model, values=["cpu", "cuda"], variable=self.var_hw, command=self.on_setting_change)
        self.opt_hw.pack(fill="x", padx=10, pady=(0, 15))

        # CPU Threads
        total_cores = mp.cpu_count()
        ctk.CTkLabel(tab_model, text=t("cpu_threads", total_cores)).pack(anchor="w", padx=10)
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
        
        ctk.CTkLabel(tab_model, text=t("model_size")).pack(anchor="w", padx=10)
        self.var_model = ctk.StringVar(value=current_model)
        self.opt_model = self._create_premium_option_menu(tab_model, values=modelos_desc, variable=self.var_model, command=self.on_setting_change)
        self.opt_model.pack(fill="x", padx=10, pady=(0, 15))

        # Selector de idioma de voz (ASR) — independiente del idioma de UI
        ctk.CTkLabel(tab_model, text=t("spoken_language_label")).pack(anchor="w", padx=10)
        asr_lang_values = ["Español", "English"]
        current_asr = self.config_data.get("asr_language", "es")
        current_asr_display = "Español" if current_asr == "es" else "English"
        self.var_asr_lang = ctk.StringVar(value=current_asr_display)
        self.opt_asr_lang = self._create_premium_option_menu(
            tab_model, values=asr_lang_values, variable=self.var_asr_lang,
            command=self._on_asr_language_change
        )
        self.opt_asr_lang.pack(fill="x", padx=10, pady=(0, 15))

        # Contexto para Whisper (initial_prompt) — carga el prompt del idioma de voz activo
        ctk.CTkLabel(tab_model, text=t("whisper_context")).pack(anchor="w", padx=10)
        self.text_whisper_prompt = ctk.CTkTextbox(tab_model, height=60)
        prompt_key = f"whisper_context_prompt_{current_asr}"
        self.text_whisper_prompt.insert("0.0", self.config_data.get(prompt_key, ""))
        self.text_whisper_prompt.pack(fill="x", padx=10, pady=(0, 15))
        self.text_whisper_prompt.bind("<FocusOut>", lambda e: self.on_setting_change())
        help_key = f"whisper_context_help_{current_asr}"
        self.lbl_whisper_context_help = ctk.CTkLabel(
            tab_model,
            text=t(help_key),
            wraplength=260,
            justify="left",
            font=ctk.CTkFont(size=10),
            text_color="#AEB8BC",
        )
        self.lbl_whisper_context_help.pack(anchor="w", padx=10, pady=(0, 0))

        # --- Sliders de Latencia ---
        ctk.CTkLabel(tab_audio, text=t("latency_control"), font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        
        # Slider Silencio
        self.lbl_silence = ctk.CTkLabel(tab_audio, text=t("silence_detection", self.config_data['silence_timeout']))
        self.lbl_silence.pack(anchor="w", padx=10)
        self.slider_silence = ctk.CTkSlider(tab_audio, from_=0.3, to=2.0, command=self.on_setting_change)
        self.slider_silence.set(self.config_data["silence_timeout"])
        self.slider_silence.pack(fill="x", padx=10, pady=(0, 10))

        # Slider Guillotina
        self.lbl_max_dur = ctk.CTkLabel(tab_audio, text=t("max_phrase_duration", self.config_data['max_chunk_duration']))
        self.lbl_max_dur.pack(anchor="w", padx=10)
        self.slider_max_dur = ctk.CTkSlider(tab_audio, from_=2.0, to=15.0, command=self.on_setting_change)
        self.slider_max_dur.set(self.config_data["max_chunk_duration"])
        self.slider_max_dur.pack(fill="x", padx=10, pady=(0, 15))

        # === SECCIÓN: Perfiles ===
        ctk.CTkLabel(tab_advanced, text=t("config_profile"), font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        current_profile_id = self.config_data.get("selected_profile_id", "custom")
        
        # Traducir los presets en la UI
        preset_labels = {
            "fast": t("profile_preset_fast_label"),
            "balanced": t("profile_preset_balanced_label"),
            "quality": t("profile_preset_quality_label"),
            "stable_streaming": t("profile_preset_stable_label"),
            "custom": t("custom")
        }
        current_profile_label = preset_labels.get(current_profile_id, t("custom"))
        profile_values = [preset_labels[pid] for pid in ["fast", "balanced", "quality", "stable_streaming"]] + [t("custom")]
        
        self.var_profile = ctk.StringVar(value=current_profile_label)
        self.opt_profile = self._create_premium_option_menu(tab_advanced, values=profile_values, variable=self.var_profile, command=self.on_profile_select)
        self.opt_profile.pack(fill="x", padx=10, pady=(0, 6))
        self.lbl_profile_desc = ctk.CTkLabel(tab_advanced, text="", justify="left", wraplength=240, text_color="#AEB8BC")
        self.lbl_profile_desc.pack(fill="x", padx=10, pady=(0, 8))
        self.lbl_pending = ctk.CTkLabel(tab_advanced, text=t("active_config"), text_color="#8BC34A")
        self.lbl_pending.pack(fill="x", padx=10, pady=(0, 12))

        # Divisor visual sutil para separar Perfiles de Ajustes Avanzados
        divider_profiles = ctk.CTkFrame(tab_advanced, height=2, fg_color="#18272c")
        divider_profiles.pack(fill="x", padx=10, pady=(5, 15))

        # Toggle de Sesión
        self.var_session = ctk.BooleanVar(value=self.config_data["continuous_session"])
        self.check_session = ctk.CTkSwitch(tab_advanced, text=t("session_persistence"), variable=self.var_session, command=self.on_setting_change)
        self.check_session.pack(anchor="w", padx=10, pady=(10, 15))

        # === TAB: Subtítulos ===

        # --- Sección: Preview ---
        ctk.CTkLabel(tab_subtitulos, text=t("style_preview"), font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=10, pady=(10, 0))
        self.frame_preview = ctk.CTkFrame(tab_subtitulos, corner_radius=8)
        self.frame_preview.pack(fill="x", padx=10, pady=(0, 8))
        self.lbl_preview_sample = ctk.CTkLabel(
            self.frame_preview,
            text=t("preview_sample_text"),
            wraplength=260,
            font=ctk.CTkFont(size=16, weight="normal"),
        )
        self.lbl_preview_sample.pack(padx=12, pady=10)

        # Style selector — MUST be before _update_preview()
        ctk.CTkLabel(tab_subtitulos, text=t("visual_style")).pack(anchor="w", padx=10, pady=(10, 0))
        self.var_style = ctk.StringVar(value=self.config_data.get("subtitle_style", "default"))
        self.opt_style = self._create_premium_option_menu(
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
            text=t("send_to_obs"),
            variable=self.var_obs_enabled,
            command=self._on_obs_toggle,
        )
        self.switch_obs_enabled.pack(anchor="w", padx=10, pady=(8, 10))

        # Mini guía de OBS
        self.frame_obs_guide = ctk.CTkFrame(tab_subtitulos, fg_color="#1a2730", corner_radius=8)
        self.frame_obs_guide.pack(fill="x", padx=10, pady=(0, 10))
        ctk.CTkLabel(
            self.frame_obs_guide,
            text=t("how_to_add_obs"),
            font=ctk.CTkFont(weight="bold", size=13),
            text_color="#64B5F6",
        ).pack(anchor="w", padx=14, pady=(10, 4))
        
        guide_steps = t("obs_guide_steps", html_path=os.path.abspath('subtitulos_obs.html'))
        
        ctk.CTkLabel(
            self.frame_obs_guide,
            text=guide_steps,
            wraplength=250,
            justify="left",
            font=ctk.CTkFont(size=11),
            text_color="#B0BEC5",
        ).pack(anchor="w", padx=14, pady=(0, 10))

        # OBS backlog policy (basic)
        ctk.CTkLabel(tab_subtitulos, text=t("obs_backlog")).pack(anchor="w", padx=10)
        
        backlog_labels = {
            t("backlog_auto"): "auto",
            t("backlog_live_only"): "live_only",
            t("backlog_send_all"): "send_all",
        }
        current_backlog_policy = next((lbl for lbl, val in backlog_labels.items() if val == self.config_data.get("subtitle_backlog_policy", "auto")), t("backlog_auto"))
        self.var_backlog_policy = ctk.StringVar(value=current_backlog_policy)
        self.opt_backlog_policy = self._create_premium_option_menu(
            tab_subtitulos,
            values=list(backlog_labels.keys()),
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
            text=t("advanced_sub_config"),
            fg_color="transparent",
            border_width=1,
            command=self._toggle_advanced_subtitles,
        )
        self.btn_toggle_subtitle_advanced.pack(fill="x", padx=10, pady=(0, 8))

        self.lbl_max_live_delay = ctk.CTkLabel(self.frame_subtitle_advanced, text=t("max_live_delay", self.config_data['subtitle_max_live_delay_sec']))
        self.lbl_max_live_delay.pack(anchor="w", padx=10)
        self.slider_max_live_delay = ctk.CTkSlider(self.frame_subtitle_advanced, from_=1.0, to=120.0, command=self.on_setting_change)
        self.slider_max_live_delay.set(self.config_data["subtitle_max_live_delay_sec"])
        self.slider_max_live_delay.pack(fill="x", padx=10, pady=(0, 8))

        self.lbl_catchup_interval = ctk.CTkLabel(self.frame_subtitle_advanced, text=t("catchup_pacing", self.config_data['subtitle_catchup_interval_sec']))
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

        self.btn_apply = ctk.CTkButton(frame_bottom, text=t("apply_changes"), height=34, fg_color="#7A4B00", hover_color="#9A6100", command=self.apply_pending_settings)
        self.btn_apply.pack(fill="x", padx=20, pady=(10, 4))
        self.btn_discard = ctk.CTkButton(frame_bottom, text=t("discard_changes"), height=28, fg_color="transparent", border_width=1, command=self.discard_pending_settings)
        self.btn_discard.pack(fill="x", padx=20, pady=(0, 6))
        self.btn_power = ctk.CTkButton(frame_bottom, text=t("start_system"), height=46, font=ctk.CTkFont(size=16, weight="bold"), command=self.toggle_system)
        self.btn_power.pack(fill="x", padx=20, pady=(0, 10))
        self.btn_diagnostics = ctk.CTkButton(
            frame_bottom,
            text="Export diagnostics",
            height=28,
            fg_color="transparent",
            border_width=1,
            command=self.export_diagnostics_report,
        )
        self.btn_diagnostics.pack(fill="x", padx=20, pady=(0, 8))

        # --- CRÉDITOS ---
        lbl_credit = ctk.CTkLabel(frame_bottom, text="LiveAudio by Plynte", text_color="#AEB8BC", cursor="hand2", font=ctk.CTkFont(size=11, weight="bold"))
        lbl_credit.pack(side="bottom", pady=(0, 5))
        lbl_credit.bind("<Button-1>", lambda e: webbrowser.open_new("https://github.com/plynte-labs/LiveAudio"))

        # Panel Derecho (estado principal, preview y debug avanzado)
        frame_der = ctk.CTkFrame(self.screen_main)
        frame_der.grid(row=0, column=1, rowspan=2, sticky="nsew", padx=10, pady=10)
        frame_der.grid_columnconfigure(0, weight=1)
        frame_der.grid_rowconfigure(3, weight=1)

        ctk.CTkLabel(frame_der, text=t("live_panel"), font=ctk.CTkFont(size=22, weight="bold")).grid(row=0, column=0, sticky="w", padx=14, pady=(12, 4))

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
            text=t("privacy_notice"),
            justify="left",
            wraplength=760,
            text_color="#D0D7DA",
        )
        self.lbl_privacy.grid(row=0, column=0, sticky="ew", padx=12, pady=(10, 2))
        self.lbl_session = ctk.CTkLabel(
            frame_privacy,
            text=t("output_folder", self.config_data['output_dir']),
            justify="left",
            wraplength=760,
            text_color="#AEB8BC",
        )
        self.lbl_session.grid(row=1, column=0, sticky="ew", padx=12, pady=(0, 10))

        frame_preview = ctk.CTkFrame(frame_der)
        frame_preview.grid(row=3, column=0, sticky="nsew", padx=10, pady=(0, 8))
        frame_preview.grid_columnconfigure(0, weight=1)
        frame_preview.grid_rowconfigure(1, weight=1)
        ctk.CTkLabel(frame_preview, text=t("last_subtitle_sent"), font=ctk.CTkFont(size=16, weight="bold")).grid(row=0, column=0, sticky="w", padx=12, pady=(10, 4))
        self.preview_text = ctk.CTkTextbox(frame_preview, height=170, font=ctk.CTkFont(size=22, weight="bold"), wrap="word")
        self.preview_text.grid(row=1, column=0, sticky="nsew", padx=12, pady=(0, 12))
        self.preview_text.insert("0.0", t("no_transcripts_yet"))
        self.preview_text.configure(state="disabled")

        frame_advanced_toggle = ctk.CTkFrame(frame_der, fg_color="transparent")
        frame_advanced_toggle.grid(row=4, column=0, sticky="ew", padx=10, pady=(0, 4))
        self.var_advanced = ctk.BooleanVar(value=False)
        self.switch_advanced = ctk.CTkSwitch(frame_advanced_toggle, text=t("show_advanced_logs"), variable=self.var_advanced, command=self.toggle_advanced_logs)
        self.switch_advanced.pack(anchor="w", padx=4, pady=4)

        self.frame_logs = ctk.CTkFrame(frame_der)
        self.frame_logs.grid_columnconfigure(0, weight=1)
        self.frame_logs.grid_rowconfigure(1, weight=1)
        ctk.CTkLabel(self.frame_logs, text=t("technical_logs"), font=ctk.CTkFont(weight="bold")).grid(row=0, column=0, sticky="w", padx=8, pady=(8, 2))
        
        self.consola = ctk.CTkTextbox(self.frame_logs, state="disabled", font=ctk.CTkFont(family="Consolas", size=13), height=180)
        self.consola.grid(row=1, column=0, sticky="nsew", padx=8, pady=(0, 8))
        
        # Validar el slider al inicio
        self._ui_ready = True
        self.refresh_profile_status()
        self.update_ui_state()
        self.set_status("audio", t("status_audio_ready"), "idle")
        self.set_status("vad", t("status_vad_idle"), "idle")
        self.set_status("asr", t("status_asr_stopped"), "idle")
        self.set_status("ws", t("status_ws_stopped"), "idle")
        self.set_status("obs", "OBS: 0 clientes" if get_language() == "es" else "OBS: 0 clients", "idle")
        self.set_status("session", t("session_not_started"), "idle")

    # --- LÓGICA DE DISPOSITIVOS DE AUDIO ---
    def _refresh_device_list(self):
        """Reescanea los dispositivos de audio del sistema."""
        try:
            self._audio_devices = list_audio_devices()
        except Exception:
            self._audio_devices = []
        
        self._device_display_list = [t("system_default")]
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
        
        # Mapear de forma dinámica las políticas de backlog traducidas a valores internos
        backlog_val = "auto"
        for lbl_key, val in [("backlog_auto", "auto"), ("backlog_live_only", "live_only"), ("backlog_send_all", "send_all")]:
            if self.var_backlog_policy.get() == t(lbl_key):
                backlog_val = val
                break
        draft["subtitle_backlog_policy"] = backlog_val
        
        draft["obs_enabled"] = self.var_obs_enabled.get()
        # Preservar prompts de ambos idiomas desde draft_config (acumula cambios),
        # luego sobrescribir el del idioma activo con el contenido actual de la UI
        draft["whisper_context_prompt_es"] = self.draft_config.get(
            "whisper_context_prompt_es",
            self.config_data.get("whisper_context_prompt_es", "")
        )
        draft["whisper_context_prompt_en"] = self.draft_config.get(
            "whisper_context_prompt_en",
            self.config_data.get("whisper_context_prompt_en", "")
        )
        asr_lang = self.draft_config.get("asr_language", self.config_data.get("asr_language", "es"))
        prompt_key = f"whisper_context_prompt_{asr_lang}"
        draft[prompt_key] = self.text_whisper_prompt.get("0.0", "end").strip()
        draft["asr_language"] = asr_lang
        draft["settings_navigation_mode"] = self.config_data.get("settings_navigation_mode", "tabs")
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
        
        # Mapear de forma dinámica el valor interno a la etiqueta de backlog traducida
        backlog_val = config.get("subtitle_backlog_policy", "auto")
        backlog_lbl = t("backlog_auto")
        for lbl_key, val in [("backlog_auto", "auto"), ("backlog_live_only", "live_only"), ("backlog_send_all", "send_all")]:
            if backlog_val == val:
                backlog_lbl = t(lbl_key)
                break
        self.var_backlog_policy.set(backlog_lbl)
        
        self.slider_max_live_delay.set(config["subtitle_max_live_delay_sec"])
        self.slider_catchup_interval.set(config["subtitle_catchup_interval_sec"])
        self.text_blacklist.delete("0.0", "end")
        self.text_blacklist.insert("0.0", config.get("blacklist", ""))
        self.var_obs_enabled.set(config.get("obs_enabled", True))
        # Cargar el prompt según el idioma de voz activo
        asr_lang = config.get("asr_language", "es")
        self.var_asr_lang.set("Español" if asr_lang == "es" else "English")
        prompt_key = f"whisper_context_prompt_{asr_lang}"
        self.text_whisper_prompt.delete("0.0", "end")
        self.text_whisper_prompt.insert("0.0", config.get(prompt_key, ""))
        help_key = f"whisper_context_help_{asr_lang}"
        self.lbl_whisper_context_help.configure(text=t(help_key))
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
            self.var_profile.set(t(f"profile_preset_{profile_id}_label"))
            self.lbl_profile_desc.configure(text=t(f"profile_preset_{profile_id}_desc"))
        elif self.var_profile.get() != t("custom"):
            self.lbl_profile_desc.configure(text=t("modified_profile_desc"))
        else:
            self.lbl_profile_desc.configure(text=t("advanced_manual_desc"))

        has_pending = any(draft.get(key) != self.config_data.get(key) for key in draft.keys())
        if has_pending:
            self.lbl_pending.configure(text=t("pending_changes_btn"), text_color="#FFC107")
            self.btn_apply.configure(state="normal")
            self.btn_discard.configure(state="normal")
        else:
            self.lbl_pending.configure(text=t("active_config"), text_color="#8BC34A")
            self.btn_apply.configure(state="disabled")
            self.btn_discard.configure(state="disabled")

    def on_profile_select(self, selected):
        if not self._ui_ready or selected == t("custom"):
            return
        current_draft = self._read_ui_config()
        has_pending = any(current_draft.get(key) != self.config_data.get(key) for key in current_draft.keys())
        if has_pending and not messagebox.askyesno(t("profile_change_warning_title"), t("profile_change_warning_msg")):
            self.refresh_profile_status()
            return

        # Encontrar de forma dinámica el profile_id que corresponde al texto traducido
        profile_id = None
        for pid in ["fast", "balanced", "quality", "stable_streaming"]:
            if selected == t(f"profile_preset_{pid}_label"):
                profile_id = pid
                break

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

    def _on_asr_language_change(self, selected):
        """Callback cuando cambia el idioma de voz (ASR)."""
        if not self._ui_ready:
            return

        new_lang_code = "es" if selected == "Español" else "en"
        prev_lang_code = self.draft_config.get("asr_language", "es")

        if new_lang_code == prev_lang_code:
            return

        # 1. Guardar el prompt actual en la clave del idioma anterior
        current_prompt = self.text_whisper_prompt.get("0.0", "end").strip()
        prev_prompt_key = f"whisper_context_prompt_{prev_lang_code}"
        self.draft_config[prev_prompt_key] = current_prompt

        # 2. Cambiar idioma ASR
        self.draft_config["asr_language"] = new_lang_code

        # 3. Cargar el prompt del nuevo idioma en la caja de texto
        new_prompt_key = f"whisper_context_prompt_{new_lang_code}"
        new_prompt = self.draft_config.get(new_prompt_key, "")
        self.text_whisper_prompt.delete("0.0", "end")
        self.text_whisper_prompt.insert("0.0", new_prompt)

        # 4. Actualizar texto de ayuda
        help_key = f"whisper_context_help_{new_lang_code}"
        self.lbl_whisper_context_help.configure(text=t(help_key))

        # 5. Disparar cambio para guardar config y notificar al motor
        self.on_setting_change()

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

        self.lbl_silence.configure(text=t("silence_detection", self.slider_silence.get()))
        self.lbl_max_dur.configure(text=t("max_phrase_duration", self.slider_max_dur.get()))
        self.lbl_max_live_delay.configure(text=t("max_live_delay", self.slider_max_live_delay.get()))
        self.lbl_catchup_interval.configure(text=t("catchup_pacing", self.slider_catchup_interval.get()))
        self.refresh_profile_status()

    def _on_style_change(self, *args):
        """Handle style change: update preview and trigger settings change."""
        self._update_preview()
        self.on_setting_change()

    def _on_nav_category_change(self, selected_category):
        """Maneja el cambio de categoría de ajustes en modo dropdown."""
        if hasattr(self, "_nav_frames"):
            for frame in self._nav_frames.values():
                frame.pack_forget()
            if selected_category in self._nav_frames:
                self._nav_frames[selected_category].pack(fill="both", expand=True, padx=5)

    def _on_welcome_nav_mode_change(self, selected_mode):
        """Maneja el cambio del modo de diseño desde la pantalla de bienvenida."""
        new_mode = "tabs" if selected_mode == t("tabs_horizontal") else "dropdown"
        self.config_data["settings_navigation_mode"] = new_mode
        save_config(self.config_data)
        self.shared_config["settings_navigation_mode"] = new_mode

    def _rebuild_main_screen_dynamic(self):
        """Reconstruye la pantalla principal de forma dinámica en tiempo de ejecución."""
        self._ui_ready = False
        if hasattr(self, "screen_main") and self.screen_main:
            self.screen_main.destroy()
        
        self.screen_main = ctk.CTkFrame(self, fg_color="transparent")
        self.build_main_screen()
        self.screen_main.grid(row=0, column=0, sticky="nsew")

    def _toggle_advanced_subtitles(self):
        """Toggle visibility of advanced subtitle settings (progressive disclosure)."""
        self._subtitle_advanced_visible = not self._subtitle_advanced_visible
        if self._subtitle_advanced_visible:
            self.frame_subtitle_advanced.pack(fill="x", padx=10, pady=(0, 8))
            self.btn_toggle_subtitle_advanced.configure(text=t("hide_advanced_sub_config"))
        else:
            self.frame_subtitle_advanced.pack_forget()
            self.btn_toggle_subtitle_advanced.configure(text=t("advanced_sub_config"))

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
        
        # Traducir los presets en la UI
        preset_labels = {
            "fast": t("profile_preset_fast_label"),
            "balanced": t("profile_preset_balanced_label"),
            "quality": t("profile_preset_quality_label"),
            "stable_streaming": t("profile_preset_stable_label"),
            "custom": t("custom")
        }

        if profile_id == "custom" and self.var_profile.get() != t("custom"):
            response = messagebox.askyesnocancel(
                t("profile_modified_title"),
                t("profile_modified_msg"),
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
                t("apply_changes"),
                t("hot_swap_confirm_msg"),
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
                self.print_log(t("log_hot_swap"))
                self.set_status("asr", t("status_asr_applying"), "warn")
                self.set_status("vad", t("status_vad_applying"), "warn")
                if not self.shared_config["continuous_session"]:
                    timestamp = datetime.datetime.now().strftime("%Y-%m-%d_%H%M%S")
                    self.current_session_dir = os.path.join(self.shared_config["output_dir"], f"session_{timestamp}")
                    self.update_session_label()
                if not self.hot_swap_engine():
                    raise RuntimeError(t("log_hot_swap_failed"))

            if previous_config.get("settings_navigation_mode") != self.config_data.get("settings_navigation_mode"):
                # Reconstruir de forma diferida para evitar warnings de destrucción durante el propio callback del botón
                self.after(50, self._rebuild_main_screen_dynamic)

            # Si el idioma cambió en la config, reconstruir la UI completa
            if previous_config.get("language") != self.config_data.get("language"):
                set_language(self.config_data.get("language") or autodetect_language())
                self.after(50, self._rebuild_ui)

            save_config(self.config_data)
            self.draft_config = copy.deepcopy(self.config_data)
            self.var_profile.set(preset_labels.get(self.config_data.get("selected_profile_id"), t("custom")))
            self.refresh_profile_status()
            self.print_log(t("log_config_applied"))
        except Exception as e:
            self.config_data = previous_config
            for k, v in self.config_data.items():
                self.shared_config[k] = v
            if self.is_running and (needs_asr_restart or needs_audio_restart):
                self.hot_swap_engine()
            messagebox.showerror(t("error_applying_title"), t("error_applying_msg", e))
            self.print_log(t("log_config_error", e))
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
        self.preview_text.insert("0.0", clean_text or t("no_transcripts_yet"))
        self.preview_text.configure(state="disabled")

    def _update_preview(self):
        """Update static preview panel based on selected style from PRESET_STYLES."""
        style_name = self.var_style.get()
        style = PRESET_STYLES.get(style_name, PRESET_STYLES["default"])
        if hasattr(self, "frame_preview"):
            self.frame_preview.configure(fg_color=style["fg_color"])
        if hasattr(self, "lbl_preview_sample"):
            self.lbl_preview_sample.configure(
                text=t("preview_sample_text"),
                text_color=style["text_color"],
                font=ctk.CTkFont(size=18, weight=style["font_weight"], family=style["font_family"]),
            )

    def update_session_label(self):
        if self.current_session_dir:
            text = t("active_session", self.current_session_dir)
            self.set_status("session", t("session_saving"), "ok")
        else:
            text = t("output_folder", self.config_data['output_dir'])
            self.set_status("session", t("session_not_started"), "idle")
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
            raw_text = event.get("text", "")
            status_key = event.get("key")
            state = event.get("state", "idle")
            
            # Buscar dinámicamente la traducción correspondiente en el diccionario español
            from utils.i18n import TRANSLATIONS
            found_key = None
            for t_key, t_val in TRANSLATIONS["es"].items():
                if t_key.startswith("status_") and t_val == raw_text:
                    found_key = t_key
                    break
            
            # Mapeos especiales fuera del prefijo status_
            if not found_key:
                if raw_text == "Sesion: guardando":
                    found_key = "session_saving"
                elif raw_text == "Sesion: sin iniciar":
                    found_key = "session_not_started"
                elif raw_text in ("Audio: dispositivo listo", "Audio: dispositivo por defecto", "Audio: loopback WASAPI"):
                    found_key = "status_audio_listening"
                    
            translated_text = t(found_key) if found_key else raw_text
            self.set_status(status_key, translated_text, state)
            
        elif event_type == "transcript":
            latency = event.get("latency")
            total_delay = event.get("total_delay")
            obs_emitted = event.get("obs_emitted", True)
            
            # El preview local en la UI siempre debe mostrarse para dar feedback
            self.set_preview(event.get("text", ""))
            
            if obs_emitted:
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

    def _collect_runtime_diagnostics(self):
        statuses = {key: label.cget("text") for key, label in self.status_labels.items()}
        queue_sizes = {
            "audio": _safe_queue_size(self.audio_queue),
            "text": _safe_queue_size(self.text_queue),
            "log": _safe_queue_size(self.log_queue),
        }
        return build_app_runtime_summary(
            {
                "is_running": self.is_running,
                "session_dir": self.current_session_dir,
                "statuses": statuses,
                "processes": {
                    "audio": self.p_audio,
                    "asr": self.p_ia,
                    "ws": self.p_ws,
                },
                "queues": queue_sizes,
            }
        )

    def export_diagnostics_report(self):
        export_dir = self.config_data.get("diagnostics_export_dir") or self.config_data.get("output_dir")
        report = build_diagnostics_report(runtime_health=self._collect_runtime_diagnostics())
        file_path = export_local_diagnostics_report(report, export_dir)
        self.print_log(f"[Diagnostics] Local-only report exported: {file_path}")
        messagebox.showinfo("Diagnostics", f"Local diagnostics exported to:\n{file_path}")

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
        # Guard de seguridad para prevenir spawn no deseado de procesos en segundo plano
        if not self.is_running:
            return False

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
                response = messagebox.askyesnocancel(t("pending_changes_title"), t("welcome_pending_msg"))
                if response is None:
                    return
                if response is True:
                    self.apply_pending_settings()
                    draft = self._read_ui_config()
                    if any(draft.get(key) != self.config_data.get(key) for key in draft.keys()):
                        return
        if not self.is_running:
            self.is_running = True
            self.btn_power.configure(text=t("stop_system"), fg_color="darkred", hover_color="red")
            
            # Limpiar consola (habilitar → borrar → deshabilitar)
            self._log_lines = []
            self.consola.configure(state="normal")
            self.consola.delete("0.0", "end")
            self.consola.configure(state="disabled")
            
            self.print_log(t("log_base_init"))
            self.set_status("audio", t("status_audio_init"), "active")
            self.set_status("vad", t("status_vad_loading"), "active")
            self.set_status("asr", t("status_asr_loading"), "active")
            self.set_status("ws", t("status_ws_initiating"), "active")
            self.set_status("obs", "OBS: 0 clientes" if get_language() == "es" else "OBS: 0 clients", "idle")
            
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
            self.btn_power.configure(text=t("start_system"), fg_color=["#3B8ED0", "#1F6AA5"], hover_color=["#36719F", "#144870"])
            self.print_log(t("log_base_stopping"))
            self.set_status("audio", t("status_audio_stopped"), "idle")
            self.set_status("vad", t("status_vad_idle"), "idle")
            self.set_status("asr", t("status_asr_stopped"), "idle")
            self.set_status("ws", t("status_ws_stopped"), "idle")
            self.set_status("obs", "OBS: 0 clientes" if get_language() == "es" else "OBS: 0 clients", "idle")
            
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
                response = messagebox.askyesnocancel(t("pending_changes_title"), t("closing_pending_msg"))
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
