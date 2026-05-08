# Tech Stack

## Runtime

- Python 3.10+.
- Desktop GUI with CustomTkinter.
- Multiprocessing architecture for audio capture, ASR inference, and UI orchestration.

## ASR And Audio

- `faster-whisper` for Whisper transcription.
- `torch` for CPU/CUDA execution support.
- `sounddevice` for microphone and WASAPI loopback capture.
- `numpy` for audio buffers.
- Silero VAD integration through the existing audio pipeline.

## Networking And OBS

- `websockets` for local WebSocket subtitle broadcast.
- `subtitulos_obs.html` as OBS Browser Source client.

## Persistence

- Local JSON config through `utils/config.py` and `config.json`.
- Session outputs under local session directories using transcript/subtitle files.

## Validation Commands

- Syntax check: `python -m compileall main.py core utils`.
- Dependency install: `pip install -r requirements.txt`.
- Manual run: `python main.py`.

## Change Control

- Tech stack changes must be documented here before implementation.
- For code tasks, run the closest available validation even when no formal test suite exists.
