import time
import os
import json
import multiprocessing as mp
from faster_whisper import WhisperModel

def asr_consumer(audio_queue: mp.Queue, text_queue: mp.Queue, log_queue: mp.Queue, shared_config: dict, session_dir: str):
    try:
        clean_model_name = shared_config["model_size"].split()[0] 
        log_queue.put(f"[IA] Cargando Whisper ({clean_model_name}) en {shared_config['device'].upper()}...")
        
        model_kwargs = {
            "model_size_or_path": clean_model_name,
            "device": shared_config["device"],
            "compute_type": "float16" if shared_config["device"] == "cuda" else "int8"
        }
        if shared_config["device"] == "cpu":
            model_kwargs["cpu_threads"] = int(shared_config["cpu_threads"])

        model = WhisperModel(**model_kwargs)
        log_queue.put("[IA] ✅ Modelo cargado y listo.")

        # --- GESTIÓN ESTRICTA DE SESIÓN ---
        os.makedirs(session_dir, exist_ok=True)
        vtt_path = os.path.join(session_dir, "subtitles.vtt")
        jsonl_path = os.path.join(session_dir, "transcript.jsonl")
        
        if not os.path.exists(vtt_path):
            with open(vtt_path, "w", encoding="utf-8") as f: f.write("WEBVTT\n\n")
            log_queue.put(f"[IA] 📝 Nueva sesión en: {session_dir}")
        else:
            log_queue.put(f"[IA] 🔄 Continuando sesión en: {session_dir}")

        while True:
            audio_chunk = audio_queue.get()
            if audio_chunk is None: break
            
            start_time = time.time()
            segments, info = model.transcribe(audio_chunk, language="es", beam_size=5, vad_filter=False, condition_on_previous_text=False)

            # Leemos la blacklist en TIEMPO REAL desde la memoria compartida
            blacklist = [w.strip().lower() for w in shared_config["blacklist"].split(",") if w.strip()]

            textos_filtrados = []
            for segment in segments:
                texto_limpio = segment.text.strip()
                if segment.no_speech_prob > 0.6 or len(texto_limpio) <= 2: continue
                if any(frase in texto_limpio.lower() for frase in blacklist): continue
                textos_filtrados.append(texto_limpio)

            texto_final = " ".join(textos_filtrados).strip()
            latency = time.time() - start_time

            if texto_final:
                log_queue.put(f"({latency:.2f}s) {texto_final}")
                
                with open(jsonl_path, "a", encoding="utf-8") as f:
                    f.write(json.dumps({"text": texto_final, "latency": latency}) + "\n")
                
                with open(vtt_path, "a", encoding="utf-8") as f:
                    f.write(f"{texto_final}\n\n")

                # Empaquetamos enviando el estilo actualizado en TIEMPO REAL
                payload = {
                    "text": texto_final,
                    "style": shared_config.get("subtitle_style", "default")
                }
                text_queue.put(payload)
                
    except Exception as e:
        log_queue.put(f"[IA ERROR] {str(e)}")