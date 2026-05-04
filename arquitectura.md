/live_asr_project
│
├── /core                 # Lógica dura (Procesos en segundo plano)
│   ├── __init__.py
│   ├── engine.py         # Consumidor ASR (Whisper) y lógica de inferencia
│   ├── audio.py          # Productor (Captura de audio, VAD, Selección de dispositivo, Auto-reconexión)
│   └── network.py        # Servidor WebSocket (Multi-cliente, Broadcast)
│
├── /gui                  # Capa de presentación (reservado para futuro)
│
├── /utils                # Herramientas transversales
│   ├── __init__.py
│   ├── config.py         # Gestor de configuración (Lectura/Escritura de config.json, Migración)
│   └── logger.py         # (Legacy) Creador de VTT y JSONL — la lógica activa está en engine.py
│
├── config.json           # Persistencia de usuario (Rutas, Blacklist, Modelo, Dispositivo de audio)
├── main.py               # Punto de entrada, GUI (CustomTkinter) y orquestador de Multiprocessing
├── subtitulos_obs.html   # Browser Source para OBS (WebSocket client, estilos visuales)
├── requirements.txt      # Dependencias del proyecto
├── .gitignore
│
└── /legacy               # Archivos de referencia y sesiones antiguas
    ├── Preguntas.md
    └── /sessions