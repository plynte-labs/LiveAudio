# SPDX-License-Identifier: MIT
import os
import json
import time
from datetime import datetime

class SessionLogger:
    def __init__(self, base_dir="sessions"):
        # Crear carpeta principal si no existe
        os.makedirs(base_dir, exist_ok=True)
        
        # Generar nombre de carpeta única para la sesión actual
        timestamp = datetime.now().strftime("%Y-%m-%d_%H%M%S")
        self.session_dir = os.path.join(base_dir, f"session_{timestamp}")
        os.makedirs(self.session_dir, exist_ok=True)
        
        # Rutas de los archivos
        self.jsonl_path = os.path.join(self.session_dir, "transcript.jsonl")
        self.vtt_path = os.path.join(self.session_dir, "subtitles.vtt")
        
        # Iniciar archivo VTT con la cabecera estándar
        with open(self.vtt_path, "w", encoding="utf-8") as f:
            f.write("WEBVTT\n\n")
            
        # Guardar metadatos de la sesión
        self._save_metadata()
        
        self.start_time = time.time()
        self.segment_index = 1

    def _save_metadata(self):
        meta_path = os.path.join(self.session_dir, "session.json")
        with open(meta_path, "w", encoding="utf-8") as f:
            json.dump({"start_time": datetime.now().isoformat(), "model": "whisper-turbo"}, f)
            
    def _format_vtt_time(self, seconds):
        """Convierte segundos a formato HH:MM:SS.mmm para VTT"""
        hours = int(seconds // 3600)
        minutes = int((seconds % 3600) // 60)
        secs = seconds % 60
        return f"{hours:02d}:{minutes:02d}:{secs:06.3f}"

    def log_segment(self, text, process_time):
        """Guarda el segmento en JSONL y actualiza el VTT"""
        if not text:
            return
            
        # Calcular marcas de tiempo relativas al inicio de la sesión
        current_time = time.time()
        end_sec = current_time - self.start_time
        # Asumimos que el inicio del segmento fue hace (tiempo de procesamiento + una constante arbitraria del buffer)
        # Para un VTT simple en tiempo real, el inicio es el final anterior.
        start_sec = max(0, end_sec - process_time - 1.5) 

        # 1. Guardar en JSONL (Estructurado para datos)
        entry = {
            "id": self.segment_index,
            "text": text,
            "latency_sec": round(process_time, 2),
            "timestamp": datetime.now().isoformat()
        }
        with open(self.jsonl_path, "a", encoding="utf-8") as f:
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
            
        # 2. Guardar en VTT (Formato de subtítulos compatible con OBS/Reproductores)
        with open(self.vtt_path, "a", encoding="utf-8") as f:
            f.write(f"{self.segment_index}\n")
            f.write(f"{self._format_vtt_time(start_sec)} --> {self._format_vtt_time(end_sec)}\n")
            f.write(f"{text}\n\n")
            
        self.segment_index += 1