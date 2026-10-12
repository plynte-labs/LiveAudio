# ADR-016 (DRAFT): Evaluación de AEC y Captura Duplex para Mitigación de Eco y Self-Barge-In

**Estado**: ⏳ En Pausa / Indecisa (Propuesta técnica evaluada)  
**Fecha**: 2026-09-21  
**Reevaluación documental**: 2026-10-10 (tras la migración híbrida Rust/Python; sin autorización de implementación)
**Módulos Afectados**: `liveaudio/core/audio.py`, `liveaudio/core/devices.py`, integración WebSocket con OpenCohost  

---

## 1. Contexto y Problema

En la evaluación original, LiveAudio procesaba audio del micrófono mediante **PortAudio (`sounddevice`) → Ring Buffer → Silero VAD → Faster-Whisper STT → WebSocket Server**. La sección 5 distingue ese contexto histórico del estado posterior a la migración.

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

Esta matriz conserva la evaluación original. Los plazos y cifras de CPU/latencia son estimaciones históricas, no mediciones del núcleo Rust ni garantías de rendimiento; los absolutos de costo o compatibilidad tampoco constituyen validación del producto actual.

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

---

## 5. Reevaluación tras la migración — 2026-10-10

**Resultado: reconsiderar RNNoise, AEC y barge-in como capacidades independientes; este ADR sigue en pausa.** Se documenta el interés del Product Owner en controles opcionales, no una aprobación del backend, de la interfaz ni de su implementación. Las secciones 1–4 preservan el razonamiento histórico; Python no impedía por definición estas funciones y Rust no las habilita automáticamente.

### 5.1 Antes y ahora

| Capacidad | Evaluación Python original | Estado híbrido Rust/Python | Aporte esperado y límite |
|---|---|---|---|
| RNNoise | Descartado como solución de eco, no como supresor de ruido. | La captura nativa ofrece un lugar para integrar DSP antes del VAD/ASR; RNNoise no está integrado. | Puede atenuar ruido y reducir falsas activaciones. No cancela por sí solo la voz del asistente y puede alterar la voz del usuario; la mejora de ASR requiere medición. |
| AEC | Integración nativa y empaquetado adicionales respecto al entorno Python. | Ya hay captura de micrófono o loopback; no captura dúplex sincronizada ni AEC. | Puede reducir el audio de bocinas que vuelve al micrófono, incluida la voz del asistente. Rust facilita la integración nativa, no resuelve la sincronización ni garantiza la cancelación. |
| Barge-in | Política conversacional asignada a OpenCohost. | El VAD Rust dispone de `SpeechStart` local; falta el contrato acústico hacia el consumidor conversacional. | Permitiría interrumpir al asistente al comenzar a hablar, sin esperar una transcripción completa. OpenCohost decide cancelar TTS y gestionar turnos; LiveAudio no envía `cancel_tts`. |

La arquitectura sigue siendo **captura/VAD en Rust + Faster-Whisper en un worker Python supervisado**, no un ASR completamente Rust. Referencias: [arquitectura WU1](migration/architecture_wu1.md), [requisitos actuales del runtime ASR](migration/rust-runtime-requirements.md), [transiciones VAD](../crates/liveaudio-vad/src/types.rs) e [integración desktop](../desktop/src-tauri/src/commands.rs). Esta revisión no mide CPU, latencia ni calidad de transcripción.

### 5.2 Qué significa la referencia de AEC

**No hace falta grabarse previamente ni entrenar el sistema con la voz propia.** La referencia es el audio digital que se está enviando, en vivo, a la salida elegida: por ejemplo, las bocinas donde OpenCohost reproduce TTS. LiveAudio necesitaría capturar esa salida por loopback mientras captura el micrófono. La referencia indica al cancelador qué señal conocida puede regresar como eco; el micrófono contiene la voz del usuario más ese retorno acústico. [WASAPI loopback, Microsoft](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording).

Seleccionar la salida correcta es sencillo para el usuario; el trabajo técnico está en combinar ambos flujos:

- Alinear formatos, canales y tiempos; compensar demora y deriva entre relojes de captura y reproducción.
- Mantener continuidad cuando no hay reproducción, distinguir silencio de pérdida de referencia y reaccionar a desconexiones o cambios de salida.
- Conservar la voz del usuario cuando habla a la vez que el asistente (*double-talk*), sin asumir cancelación perfecta.
- Cubrir la ruta realmente audible: audio de un teléfono u otra bocina ajena a la salida seleccionada no tiene referencia en ese loopback.

Se propone procesamiento **local y efímero**, sin nuevas grabaciones ni persistencia de la referencia. Esto no modifica las opciones existentes de almacenamiento de sesiones. AEC y RNNoise procesarían la entrada de LiveAudio: **no limpiarían automáticamente el micrófono que OBS o Discord capturan por separado**.

### 5.3 Controles opcionales propuestos

| Control | Propietario | Comportamiento propuesto, aún no implementado |
|---|---|---|
| Reducción de ruido | LiveAudio | Switch RNNoise independiente, inicialmente apagado; comparar voz/VAD/ASR con y sin procesamiento antes de adoptar un valor predeterminado. |
| Cancelación de eco | LiveAudio | Switch AEC más selección de salida de referencia; mostrar estado efectivo, no disponible o referencia perdida. Un switch encendido no equivale a procesamiento operativo. |
| Interrumpir al hablar | OpenCohost | Switch de política conversacional basado en eventos acústicos de LiveAudio. Probar primero con audífonos; con bocinas, el eco puede disparar interrupciones falsas sin AEC eficaz. |

El orden sugerido para una futura evaluación es RNNoise, barge-in con audífonos y AEC con bocinas. Es una recomendación, no un roadmap aprobado. El orden final del DSP, sus tasas de muestreo y cambios de configuración requieren diseño y pruebas propios.

### 5.4 Alternativas nativas y empaquetado

| Alternativa AEC | Ventaja | Condición / trade-off |
|---|---|---|
| DSP integrado, por ejemplo WebRTC AEC | Mayor control del pipeline y posibilidad de compartir backend entre plataformas. | Incorporar y distribuir el backend elegido, su referencia de reproducción y adaptación de formato/tiempo; validar por plataforma. |
| AEC nativo de Windows | Puede aprovechar el procesamiento del sistema en dispositivos compatibles. | La disponibilidad depende del sistema y del endpoint; detectar soporte y reportar indisponibilidad. No tratar `IAcousticEchoCancellationControl` como capacidad universal. |

El empaquetado Rust/Tauri existente reduce la novedad de distribuir código nativo, pero **no incorpora automáticamente RNNoise/WebRTC ni elimina Python del ASR**. Antes de anunciar estas funciones como distribuibles, comprobar:

1. Backend/versiones y modelo RNNoise seleccionado, ABI/arquitectura, enlace estático o bibliotecas dinámicas y recursos necesarios.
2. Licencias y avisos de redistribución de cada dependencia/modelo, además de su inclusión reproducible en el bundle.
3. Instalador NSIS y distribución portable con el mismo DSP, sus recursos y carga de bibliotecas fuera del checkout de desarrollo.
4. Instalación limpia en CPU, sin GPU obligatoria para este DSP, y cualquier ruta CUDA anunciada para ASR; comprobar desactivación y errores sin perder el servicio.

**Referencias de empaquetado verificables:**

- [Requisitos actuales del runtime Rust/ASR](migration/rust-runtime-requirements.md): frontera Python, aprovisionamiento explícito y límites de la evidencia en máquinas limpias.
- [Snapshot histórico de empaquetado WU8](migration/packaging_wu8.md): contexto de instalador/portable; sus resultados previos no prueban distribución de DSP nuevo.
- [Configuración Tauri](../desktop/src-tauri/tauri.conf.json), [manifiesto desktop](../desktop/src-tauri/Cargo.toml) y [manifiesto de audio](../crates/liveaudio-audio/Cargo.toml): configuración y dependencias del checkout actual.
- [Herramienta de empaquetado Tauri](../packaging/package_tauri_release.py): referencia de ensamblado/validación existente, no evidencia de ejecución en esta revisión.
- [Empaquetado del launcher Python anterior](PACKAGING_AND_UPDATES.md): antecedente, no descripción del runtime Rust actual.

**Fuentes primarias del procesamiento:** [RNNoise](https://github.com/xiph/rnnoise), [API de procesamiento de audio WebRTC](https://webrtc.googlesource.com/src/+/refs/heads/main/api/audio/audio_processing.h), [WASAPI y control de AEC](https://learn.microsoft.com/en-us/windows/win32/coreaudio/wasapi) y [ejemplo oficial de AEC de Windows](https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/AcousticEchoCancellation/README.md).

### 5.5 Evidencia pendiente antes de implementación o cierre

Una futura propuesta deberá definir backend y plataforma, contrato acústico con OpenCohost, comportamiento al perder referencia y persistencia de configuración. La aceptación requerirá comparar VAD/ASR con ruido y voz limpia, eco con bocinas, double-talk, silencio, cambios de dispositivo y sesiones largas; medir CPU/latencia y verificar los artefactos instalador/portable reales. Nada de ello queda validado por esta actualización documental.
