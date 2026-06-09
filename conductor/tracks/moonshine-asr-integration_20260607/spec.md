# Spec: Experimental Moonshine ASR Integration

## 1. Objetivo
Evaluar y realizar una integración experimental del modelo de ASR "Moonshine" (245M) de Resonant como una alternativa opcional a Whisper para reducir la latencia de primer token y el uso de recursos en LiveAudio.

## 2. Metodología de Research
1. **Validación del Modelo:** Probar el rendimiento y la precisión de la transcripción en español e inglés en un script aislado (`tests/test_moonshine.py`).
2. **Evaluación de ONNX Runtime:** Analizar el impacto de empaquetar ONNX Runtime junto a la distribución portable.
3. **Control de VRAM:** Comparar el consumo de VRAM de la GPU frente a `Faster-Whisper` (small).

## 3. Arquitectura Propuesta
- Añadir un selector en la sección "Ajustes Avanzados" de LiveAudio para alternar el motor de inferencia: `Whisper (CTranslate2)` o `Moonshine (ONNX)`.
- Diseñar un wrapper abstracto de inferencia para intercambiar los motores en `core/engine.py` de forma limpia.
