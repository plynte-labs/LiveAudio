# ADR-016 (DRAFT): Evaluación de AEC y Captura Duplex para Mitigación de Eco y Self-Barge-In

**Estado**: ⏳ En Pausa / Indecisa (Propuesta técnica evaluada)  
**Fecha**: 2026-09-21  
**Módulos Afectados**: `liveaudio/core/audio.py`, `liveaudio/core/devices.py`, integración WebSocket con OpenCohost  

---

## 1. Contexto y Problema

LiveAudio procesa audio del micrófono mediante un pipeline compuesto por **PortAudio (`sounddevice`) → Ring Buffer → Silero VAD → Faster-Whisper STT → WebSocket Server**.

Al utilizar bocinas abiertas (sin audífonos):
1. El audio reproducido por el sistema (TTS de Kira en OpenCohost, música de Spotify, sonido de videojuegos, llamadas) es capturado acústicamente por el micrófono físico.
2. **Silero VAD** detecta presencia de voz humana independientemente de su procedencia física.
3. **Whisper STT** transcribe tanto la voz del streamer como el audio proveniente de las bocinas.
4. En **OpenCohost**, esto provoca un fenómeno de **self-barge-in**: el sistema detecta que "el usuario está hablando", cuando en realidad el micrófono está escuchando a la propia Kira, interrumpiendo su generación o introduciendo alucinaciones en el prompt del LLM. En **OBS**, los subtítulos muestran letras de canciones o diálogos de juegos.

---

## 2. Decisiones Arquitectónicas Consolidadas

| Decisión | Justificación |
|---|---|
| **Separación estricta de responsabilidades** | LiveAudio es responsable exclusivamente de la **percepción acústica** (captura, limpieza, VAD, STT). OpenCohost es dueño exclusivo de la **política conversacional** (barge-in, cancelación de TTS, turn-taking). LiveAudio jamás enviará eventos imperativos como `cancel_tts`. |
| **Descarte de RNNoise como solución de eco** | RNNoise es un **supresor de ruido monoaural (NS)** diseñado para atenuar ruidos estacionarios/mecánicos (ventiladores, tecleo) preservando voz humana. Al carecer de canal de referencia, RNNoise no puede cancelar música cantada ni voz reproducida por bocinas. |
| **Inviabilidad de Windows Native AEC directo en Python** | PortAudio (`sounddevice`) no expone la interfaz COM `IAcousticEchoCancellationControl` de Windows 11. Activar el DSP nativo de Windows requeriría desarrollar y mantener extensiones C++/WinRT específicas para la plataforma. |

---

## 3. Matriz de Propuestas Evaluadas

### Opción A: Cancelación Acústica por DSP en LiveAudio (Plan Original)
* **Mecanismo**: Captura simultánea de micrófono + loopback WASAPI en LiveAudio, procesados mediante un backend adaptativo (WebRTC AEC3).
* **Ventajas**:
  - Limpia tanto la voz de Kira como la música y los juegos de la PC antes de que lleguen a Silero VAD y Whisper.
* **Desventajas / Riesgos**:
  - **Complejidad de ingeniería**: 2 a 4 semanas de desarrollo.
  - **Dependencias nativas**: Obliga a compilar y empaquetar binarios C++ para Windows y Linux, rompiendo la portabilidad del entorno Python actual.
  - **Clock Drift**: El reloj de captura y el reloj de render divergen con el tiempo; el loopback WASAPI (usualmente a 48 kHz estéreo) entra en starvation si no hay audio reproduciéndose.
  - **Impacto en recursos**: +15% a +25% de uso continuo de CPU y +30 ms a +60 ms de latencia acumulada en el pipeline de captura.

### Opción B: Filtrado de Eco por Texto en OpenCohost + PTT Manual (Recomendación Minimalista)
* **Mecanismo**: LiveAudio permanece sin cambios en su pipeline de audio. OpenCohost compara las transcripciones entrantes con el buffer de su propia voz generada (`state == SPEAKING`). Si la similitud léxica supera el umbral (e.g. > 80%), el evento se descarta silenciosamente. La interrupción de Kira se reserva al Push-to-Talk (PTT).
* **Ventajas**:
  - **Costo CERO**: 0 dependencias C++, 0% de CPU extra, 0 ms de latencia en LiveAudio.
  - 100% compatible con Windows, Linux y macOS.
* **Desventajas / Trade-offs**:
  - Solo neutraliza el eco de Kira. Si el streamer reproduce música con voces o juegos por bocinas, Whisper transcribirá ese audio si el micrófono lo capta.

### Opción C: Delegación a Dispositivos Virtuales del Sistema (NVIDIA Broadcast / Krisp)
* **Mecanismo**: Para usuarios con bocinas que requieran cancelación avanzada de ruido y música de fondo, se delega el procesamiento a herramientas a nivel de sistema operativo (como NVIDIA Broadcast o RTX Voice). LiveAudio simplemente captura del micrófono virtual resultante.
* **Ventajas**:
  - Cero código en LiveAudio; no requiere dependencias CUDA en nuestro repositorio.
* **Desventajas**:
  - Requiere hardware compatible (GPU NVIDIA) o instalación de software externo por parte del usuario.

---

## 4. Plan de Acción y Roadmap Futuro

1. **Estado Actual (Pausa Decisional)**:
   - No se implementa duplex capture ni AEC en LiveAudio en este momento para evitar deuda técnica y sobrecoste de CPU.
   - El caso de uso estándar recomendado sigue siendo el uso de **audífonos** para streaming.
2. **Habilitación en OpenCohost (Fase Próxima)**:
   - Implementar el ciclo de vida formal de TTS con `utterance_id` y supresión de transcripciones duplicadas por coincidencia de texto.
3. **Criterio de Reactivación del Track AEC**:
   - Solo se abrirá un track SDD formal (`conductor-newTrack`) para AEC si las pruebas de campo demuestran que una proporción crítica de usuarios opera obligatoriamente con bocinas abiertas y sin soluciones de micrófono virtual a nivel de driver.
