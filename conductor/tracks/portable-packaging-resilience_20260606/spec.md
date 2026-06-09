# Spec: Portable Packaging, CUDA 12 Fixes & Application Resilience

## 1. Problema y Motivación
LiveAudio requería ser distribuido como un paquete autónomo portátil (.zip) para Windows sin requerir dependencias en el sistema operativo del usuario. Al iniciar en modo CUDA, la inferencia de Whisper fallaba debido a que no encontraba `cublas64_12.dll`, ya que PyTorch (cu118) instalaba bibliotecas CUDA 11 incompatibles con ctranslate2 4.x. Adicionalmente, el ciclo de vida de la aplicación carecía de un gestor de errores inesperados y actualizaciones asíncronas seguras para producción.

## 2. Requerimientos Técnicos
- **Portable CUDA 12.1 Target**: Migrar PyTorch a `cu121` en `build_portable.py` para asegurar la provisión nativa de las bibliotecas CUDA 12.
- **Inyección de PATH**: Asegurar que las bibliotecas DLL ubicadas en `python/Lib/site-packages/torch/lib` se inyecten de forma dinámica al `PATH` de Windows al usar los lanzadores `.bat` y el launcher binario C# `.exe` (usando `EnvironmentVariables["PATH"]`).
- **Resilience - Crash Handler**: Interceptar cualquier excepción no controlada en el hilo de la UI y del backend para mostrar una ventana premium nativa de error. Evitar el envío silencioso a servidores externos por privacidad y seguridad, proveyendo en su lugar un flujo de copia y redirección manual estructurado a GitHub Issues.
- **Resilience - Updater**: Buscar actualizaciones de forma asíncrona en un hilo background contactando a la API de GitHub Releases. Evitar el spam del endpoint limitando la verificación a una consulta cada 24 horas (`last_update_check` en la configuración) y notificar al usuario mediante un banner verde premium integrado en la UI principal.

## 3. Arquitectura del Sistema
```
[LiveAudio Launcher (.exe/.bat)]
               | (Inyecta torch\lib en PATH)
               v
     [main.py (Bootloader)]
      + sys.excepthook (install_crash_handler)
               |
               +---> [check_for_updates_async] ---> [GitHub Releases API] (Cada 24hs)
```
- **Fallas interceptadas**: `sys.excepthook` redirecciona excepciones a `utils/crash_handler.py`.
- **Detección de actualizaciones**: `utils/updater.py` gestiona la lógica asíncrona de parsing semántico (`vX.Y.Z`).
