# Product Definition

## Initial Concept

LiveAudio is a local-first real-time ASR desktop tool for streamers and content creators. It captures microphone or system audio, transcribes speech locally with Whisper/faster-whisper, and publishes live subtitles to OBS through WebSocket/browser-source integrations.

## Product Goals

- Keep audio transcription and session data local by default.
- Provide reliable live subtitles for OBS without flooding viewers after freezes or reconnects.
- Make configuration understandable for non-technical users through profiles, safe defaults, and clear warnings.
- Preserve advanced controls for tuning latency, VAD behavior, device selection, model size, CUDA/CPU execution, and subtitle output.
- Recover gracefully from device disconnects, GPU pressure, ASR stalls, and long streaming sessions.

## Target Users

- Streamers who need local subtitles in OBS.
- Creators who want session transcripts without cloud processing.
- Power users who tune latency, ASR model size, and GPU/CPU behavior for their machine.

## Product Boundaries

- No cloud transcription by default.
- User configuration is local and project-specific.
- OBS/browser-source behavior must avoid unsafe local file or WebSocket exposure.
- Feature work should remain traceable through requirements, implementation notes, and Engram memory.
