# Requerimientos Técnicos: LiveAudio-Companion (Copiloto IA)

## 1. Concepto General
Un agente de Inteligencia Artificial diseñado para acompañar al streamer durante las transmisiones, con especial énfasis en la gestión de escenas de espera (BRB - Be Right Back), interacción con el chat y mantenimiento de la retención de audiencia mediante contexto en tiempo real.

## 2. Arquitectura de Integración
El proyecto funcionará como un cliente independiente que consume datos de **LiveAudio** y **OBS**.

*   **Entrada de Voz (Contexto):** Cliente WebSocket conectado a `LiveAudio` (`ws://127.0.0.1:8765`).
*   **Entrada de Chat:** Cliente IRC para Twitch o integración con API de YouTube.
*   **Control de Estado:** Conexión a `OBS WebSocket` para detectar cambios de escena (ej. cambio a escena "Ya vuelvo").
*   **Salida de Audio:** Inyección de audio en un cable virtual (VB-Audio) o salida directa a dispositivo de audio monitorizado por OBS.

## 3. Requerimientos Funcionales

### F1: Cerebro (LLM Local)
*   Uso de un modelo de lenguaje cuantizado (4-bit/5-bit) para ejecución local.
*   Modelos recomendados: **Llama-3-8B**, **Mistral-7B** o **Phi-3-mini**.
*   Motor de inferencia: **Ollama** (vía API local) o **Llama.cpp**.
*   **Prompt System:** Configuración de personalidad, límites de respuesta y conocimiento sobre el streamer.

### F2: Memoria de Contexto
*   Capacidad de almacenar los últimos *N* minutos de transcripción de LiveAudio para responder preguntas sobre lo que el streamer acaba de decir.
*   Historial de chat reciente para mantener la coherencia en las conversaciones.

### F3: Voz (TTS Local)
*   **Piper TTS:** Para máxima velocidad y bajo consumo de CPU.
*   **XTTSv2:** (Opcional) Para clonación de voz si se cuenta con una GPU dedicada potente.
*   Control de entonación y velocidad dinámico.

### F4: Modos de Operación
1.  **Modo BRB (Activo):** El agente responde proactivamente al chat cuando el streamer no está.
2.  **Modo Copiloto (Pasivo):** El agente solo responde cuando se le menciona directamente (ej. `@IA_Bot`).
3.  **Modo Resumen:** Capacidad de generar un resumen de "lo que te perdiste" para usuarios que acaban de llegar.

## 4. Requerimientos No Funcionales
*   **Latencia:** El tiempo de respuesta (Pensar + Hablar) no debe superar los 2-3 segundos.
*   **Privacidad:** Procesamiento 100% local. Nada de lo que se diga en el stream debe enviarse a nubes externas (OpenAI, Anthropic, etc.).
*   **Recursos:** El sistema debe ser capaz de convivir con un juego y el motor ASR sin causar *stuttering* (tirones).

## 5. Stack Tecnológico Sugerido
*   **Lenguaje:** Python 3.10+
*   **Conectividad:** `websockets`, `python-twitch-irc`, `obs-websocket-py`.
*   **Inferencia LLM:** `ollama-python` o `langchain`.
*   **Inferencia TTS:** `piper-tts`.
*   **Interfaz:** (Opcional) Pequeño dashboard en `CustomTkinter` para monitorear lo que la IA está "pensando".

## 6. Siguientes Pasos de Implementación
1.  **Módulo de Escucha:** Script que se conecte a LiveAudio y guarde la transcripción en un buffer circular de memoria.
2.  **Módulo de Chat:** Conexión básica a Twitch para leer mensajes.
3.  **Módulo de Lógica:** Enviar el contexto (Voz + Chat) al LLM local y recibir respuesta.
4.  **Módulo de Voz:** Pasar la respuesta de texto a audio.
