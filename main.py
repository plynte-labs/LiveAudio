import os
import queue
import datetime
import multiprocessing as mp
import customtkinter as ctk
import torch
from tkinter import filedialog

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


class LiveASRApp(ctk.CTk):
    def __init__(self):
        super().__init__()
        self.title("Plynte LiveAudio")
        self.geometry("1200x800")
        
        # --- DEFENSA 1: Manejo del botón 'X' de Windows ---
        self.protocol("WM_DELETE_WINDOW", self.on_closing)
        
        self.config_data = load_config()
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
        
        ctk.CTkLabel(frame_izq, text="Ajustes del Motor", font=ctk.CTkFont(size=20, weight="bold")).pack(pady=20, padx=20)

        # === SECCIÓN: Dispositivo de Audio ===
        ctk.CTkLabel(frame_izq, text="Dispositivo de Audio:", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=20, pady=(10, 0))
        
        # Frame para el selector + botón refresh
        frame_device = ctk.CTkFrame(frame_izq, fg_color="transparent")
        frame_device.pack(fill="x", padx=20, pady=(0, 5))
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
        ctk.CTkLabel(frame_izq, text="Hardware:", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=20, pady=(15, 0))
        self.var_hw = ctk.StringVar(value=self.config_data["device"])
        self.opt_hw = ctk.CTkSegmentedButton(frame_izq, values=["cpu", "cuda"], variable=self.var_hw, command=self.on_setting_change)
        self.opt_hw.pack(fill="x", padx=20, pady=(0, 15))

        # CPU Threads
        total_cores = mp.cpu_count()
        ctk.CTkLabel(frame_izq, text=f"Hilos CPU (Max {total_cores}):").pack(anchor="w", padx=20)
        self.slider_threads = ctk.CTkSlider(frame_izq, from_=1, to=total_cores, number_of_steps=total_cores-1, command=self.on_setting_change)
        self.slider_threads.set(self.config_data["cpu_threads"])
        self.slider_threads.pack(fill="x", padx=20, pady=(0, 15))

        # Modelo Selector (Nombres Descriptivos)
        modelos_desc = [
            "tiny (Más rápido, baja precisión)", 
            "base (Rápido)", 
            "small (Balance CPU)", 
            "turbo (Máxima precisión GPU)"
        ]
        # Asegurar que el modelo guardado exista en la lista visual
        current_model = next((m for m in modelos_desc if m.startswith(self.config_data["model_size"].split()[0])), modelos_desc[2])
        
        ctk.CTkLabel(frame_izq, text="Tamaño del Modelo:").pack(anchor="w", padx=20)
        self.var_model = ctk.StringVar(value=current_model)
        self.opt_model = ctk.CTkOptionMenu(frame_izq, values=modelos_desc, variable=self.var_model, command=self.on_setting_change)
        self.opt_model.pack(fill="x", padx=20, pady=(0, 15))

        # --- Sliders de Latencia ---
        ctk.CTkLabel(frame_izq, text="Control de Latencia (Ritmo):", font=ctk.CTkFont(weight="bold")).pack(anchor="w", padx=20, pady=(10, 0))
        
        # Slider Silencio
        self.lbl_silence = ctk.CTkLabel(frame_izq, text=f"Detección de Silencio: {self.config_data['silence_timeout']}s")
        self.lbl_silence.pack(anchor="w", padx=20)
        self.slider_silence = ctk.CTkSlider(frame_izq, from_=0.3, to=2.0, command=self.on_setting_change)
        self.slider_silence.set(self.config_data["silence_timeout"])
        self.slider_silence.pack(fill="x", padx=20, pady=(0, 10))

        # Slider Guillotina
        self.lbl_max_dur = ctk.CTkLabel(frame_izq, text=f"Guillotina (Max Audio): {self.config_data['max_chunk_duration']}s")
        self.lbl_max_dur.pack(anchor="w", padx=20)
        self.slider_max_dur = ctk.CTkSlider(frame_izq, from_=2.0, to=15.0, command=self.on_setting_change)
        self.slider_max_dur.set(self.config_data["max_chunk_duration"])
        self.slider_max_dur.pack(fill="x", padx=20, pady=(0, 15))

        # Toggle de Sesión
        self.var_session = ctk.BooleanVar(value=self.config_data["continuous_session"])
        self.check_session = ctk.CTkSwitch(frame_izq, text="Mantener sesión en Hot-Swap", variable=self.var_session, command=self.on_setting_change)
        self.check_session.pack(anchor="w", padx=20, pady=(0, 15))

        # Estilos visuales
        ctk.CTkLabel(frame_izq, text="Estilo Visual en OBS:").pack(anchor="w", padx=20)
        self.var_style = ctk.StringVar(value=self.config_data.get("subtitle_style", "default"))
        self.opt_style = ctk.CTkOptionMenu(
            frame_izq, 
            values=["default", "karaoke", "neon"], 
            variable=self.var_style, 
            command=self.on_setting_change
        )
        self.opt_style.pack(fill="x", padx=20, pady=(0, 15))

        # Blacklist
        ctk.CTkLabel(frame_izq, text="Filtro Anti-Alucinaciones:").pack(anchor="w", padx=20)
        self.text_blacklist = ctk.CTkTextbox(frame_izq, height=80)
        self.text_blacklist.insert("0.0", self.config_data["blacklist"])
        self.text_blacklist.pack(fill="x", padx=20, pady=(0, 15))
        # <FocusOut> guarda solo cuando sales del cuadro de texto, ahorrando RAM/Disco
        self.text_blacklist.bind("<FocusOut>", lambda e: self.on_setting_change()) 

        # Botón Principal — en un frame fijo debajo del scroll
        frame_bottom = ctk.CTkFrame(self.screen_main, fg_color="transparent", height=80)
        frame_bottom.grid(row=1, column=0, sticky="ew")
        
        self.btn_power = ctk.CTkButton(frame_bottom, text="INICIAR SISTEMA", height=50, font=ctk.CTkFont(size=16, weight="bold"), command=self.toggle_system)
        self.btn_power.pack(fill="x", padx=20, pady=15)

        # Panel Derecho (Consola)
        frame_der = ctk.CTkFrame(self.screen_main)
        frame_der.grid(row=0, column=1, rowspan=2, sticky="nsew", padx=10, pady=10)
        
        self.consola = ctk.CTkTextbox(frame_der, state="disabled", font=ctk.CTkFont(family="Consolas", size=14))
        self.consola.pack(fill="both", expand=True, padx=5, pady=5)
        
        # Validar el slider al inicio
        self.update_ui_state()

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

    def on_device_change(self, *args):
        """Callback cuando se cambia el dispositivo de audio."""
        selected = self.var_device.get()
        
        if selected == self._device_display_list[0]:
            # "Por defecto del sistema"
            self.config_data["audio_device"] = None
        else:
            # Buscar el dispositivo por su display name
            device = next((d for d in self._audio_devices if d["display"] == selected), None)
            if device:
                self.config_data["audio_device"] = device
        
        save_config(self.config_data)
        
        # Actualizar memoria compartida
        self.shared_config["audio_device"] = self.config_data["audio_device"]
        
        # Si está corriendo, reiniciar el productor de audio
        if self.is_running:
            self.print_log(f"\n[Sistema] 🔄 Cambiando dispositivo de audio...")
            self.hot_swap_engine()

    # --- LÓGICA DE CONTROL ---
    def update_ui_state(self):
        """Bloquea o desbloquea elementos de la UI según la selección"""
        if self.var_hw.get() == "cuda":
            self.slider_threads.configure(state="disabled", button_color="gray")
        else:
            self.slider_threads.configure(state="normal", button_color=["#3B8ED0", "#1F6AA5"])

    def on_setting_change(self, *args):
        self.update_ui_state() 

        self.lbl_silence.configure(text=f"Detección de Silencio: {self.slider_silence.get():.1f}s")
        self.lbl_max_dur.configure(text=f"Guillotina (Max Audio): {self.slider_max_dur.get():.1f}s")
        

        self.config_data["silence_timeout"] = round(self.slider_silence.get(), 1)
        self.config_data["max_chunk_duration"] = round(self.slider_max_dur.get(), 1)
        
        # Guardar en local
        self.config_data["device"] = self.var_hw.get()
        self.config_data["model_size"] = self.var_model.get()
        self.config_data["cpu_threads"] = int(self.slider_threads.get())
        self.config_data["continuous_session"] = self.var_session.get()
        self.config_data["blacklist"] = self.text_blacklist.get("0.0", "end").strip()
        self.config_data["subtitle_style"] = self.var_style.get()
        save_config(self.config_data)

        # Detectar si el cambio requiere reiniciar la tarjeta gráfica
        needs_hard_restart = (
            self.shared_config["device"] != self.config_data["device"] or
            self.shared_config["model_size"] != self.config_data["model_size"] or
            self.shared_config["cpu_threads"] != self.config_data["cpu_threads"] 
        )

        needs_audio_restart = (
            self.shared_config["silence_timeout"] != self.config_data["silence_timeout"] or
            self.shared_config["max_chunk_duration"] != self.config_data["max_chunk_duration"]
        )

        # Actualizar la memoria compartida (Aplica estilos y blacklist al instante sin reiniciar)
        for k, v in self.config_data.items():
            self.shared_config[k] = v

        # --- DEFENSA 3: Hot-Swap Inteligente ---
        if self.is_running and (needs_hard_restart or needs_audio_restart):
            self.print_log("\n[Sistema] 🔄 Cambio de hardware detectado. Reiniciando Motor...")
            
            # Si NO queremos mantener la sesión, creamos una nueva ruta
            if not self.shared_config["continuous_session"]:
                timestamp = datetime.datetime.now().strftime("%Y-%m-%d_%H%M%S")
                self.current_session_dir = os.path.join(self.shared_config["output_dir"], f"session_{timestamp}")
                
            self.hot_swap_engine()

    def print_log(self, msg):
        self.consola.configure(state="normal")
        self.consola.insert("end", msg + "\n")
        self.consola.see("end")
        self.consola.configure(state="disabled")

    def process_logs(self):
        while True:
            try:
                msg = self.log_queue.get_nowait()
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

    def toggle_system(self):
        if not self.is_running:
            self.is_running = True
            self.btn_power.configure(text="DETENER SISTEMA", fg_color="darkred", hover_color="red")
            
            # Limpiar consola (habilitar → borrar → deshabilitar)
            self.consola.configure(state="normal")
            self.consola.delete("0.0", "end")
            self.consola.configure(state="disabled")
            
            self.print_log("[Sistema] Iniciando servicios base...")
            
            # Generar carpeta principal de la sesión
            timestamp = datetime.datetime.now().strftime("%Y-%m-%d_%H%M%S")
            self.current_session_dir = os.path.join(self.shared_config["output_dir"], f"session_{timestamp}")
            
            # Recrear cola de texto para evitar pipes corruptos entre sesiones
            self.text_queue = mp.Queue(maxsize=QUEUE_MAXSIZE)
            
            self.p_ws = mp.Process(target=run_ws_server, args=(self.text_queue,), daemon=True)
            self.p_ws.start()
            
            self.hot_swap_engine()
        else:
            self.is_running = False
            self.btn_power.configure(text="INICIAR SISTEMA", fg_color=["#3B8ED0", "#1F6AA5"], hover_color=["#36719F", "#144870"])
            self.print_log("[Sistema] Apagando todos los servicios...")
            
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

    def on_closing(self):
        """Maneja el evento de cerrar la ventana (X) para evitar procesos zombies"""
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