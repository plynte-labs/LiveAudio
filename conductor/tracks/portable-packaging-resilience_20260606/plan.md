# Plan: Portable Packaging, CUDA 12 Fixes & Application Resilience

Este plan detalla el checklist de implementación y la posterior verificación de los cambios realizados para cerrar el track con éxito.

## Checklist de Tareas

- [x] **Fase 1: Corrección de Dependencias y PATH (CUDA 12)**
  - [x] Modificar la URL de instalación de PyTorch de `cu118` a `cu121` en `build_portable.py`.
  - [x] Inyectar la ruta de DLLs de PyTorch (`torch/lib`) al `PATH` en `LiveAudio.bat`.
  - [x] Inyectar la ruta de DLLs de PyTorch al `PATH` en `LiveAudio_Debug.bat`.
  - [x] Inyectar la ruta de DLLs de PyTorch en el entorno de inicio del proceso hijo en C# (`Launcher.cs`/`build_portable.py`).

- [x] **Fase 2: Robustez y Resiliencia (Crash Handler & Updater)**
  - [x] Crear el módulo `utils/crash_handler.py` con una interfaz de reporte manual de errores.
  - [x] Crear el módulo `utils/updater.py` con la lógica de chequeo semántico asíncrono.
  - [x] Registrar la variable de control `last_update_check` en `utils/config.py`.
  - [x] Integrar `install_crash_handler` al arranque de `main.py`.
  - [x] Integrar `check_for_updates_async` e inyectar el banner verde de alerta en la UI izquierda de ajustes en `main.py`.

- [x] **Fase 3: Construcción y Validación**
  - [x] Compilar el build portable de cero seleccionando el modo GPU CUDA.
  - [x] Validar que `LiveAudio_Debug.bat` inicialice de forma satisfactoria levantando Whisper en modo CUDA con aceleración por hardware.
  - [x] Validar que el fallback a CPU siga funcionando si la GPU está ocupada o no responde.

## Pruebas de Verificación
- **Inferencia CUDA**: Verificada en GPU RTX 3060, tiempos de procesamiento ASR de ~0.21s a ~0.40s.
- **Importación de módulos**: Sin fallas en el entorno portátil embebido.
