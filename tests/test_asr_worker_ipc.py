# SPDX-License-Identifier: MIT
"""Unit tests for Faster-Whisper Python Interop Worker IPC Protocol (WU1)."""

import base64
import json
import time
from unittest.mock import MagicMock, patch

import numpy as np
import pytest

from liveaudio.service.asr_worker_protocol import (
    PROTOCOL_VERSION,
    DownloadProgressPayload,
    ErrorPayload,
    InitPayload,
    MessageEnvelope,
    ModelSwapPayload,
    PongPayload,
    ReadyPayload,
    RustCommand,
    SegmentResult,
    ShutdownPayload,
    StatusPayload,
    TranscribePayload,
    TranscriptionResultPayload,
    UpdateConfigPayload,
    WordTiming,
    WorkerEvent,
    decode_audio_base64,
    encode_audio_base64,
    parse_rust_command,
)
from liveaudio.service.asr_worker import ASRWorker


class TestIPCProtocolCodec:
    """Tests for protocol serialization and deserialization."""

    def test_parse_init_command(self):
        raw_json = json.dumps({
            "version": 1,
            "seq": 10,
            "session_id": "sess-test",
            "attempt_id": 2,
            "correlation_id": "corr-101",
            "timestamp_ms": 1775520000000,
            "cmd": "init",
            "payload": {
                "model_name": "large-v3-turbo",
                "device": "cuda",
                "device_index": 0,
                "compute_type": "float16",
                "cpu_threads": 4,
                "language": "es",
                "beam_size": 5,
                "auto_cpu_fallback": True,
            }
        })
        cmd = parse_rust_command(raw_json)
        assert cmd is not None
        assert cmd.cmd == "init"
        assert cmd.envelope.version == 1
        assert cmd.envelope.seq == 10
        assert cmd.envelope.session_id == "sess-test"
        assert cmd.envelope.attempt_id == 2
        assert cmd.envelope.correlation_id == "corr-101"
        assert isinstance(cmd.payload, InitPayload)
        assert cmd.payload.model_name == "large-v3-turbo"
        assert cmd.payload.device == "cuda"
        assert cmd.payload.auto_cpu_fallback is True

    def test_parse_transcribe_command(self):
        # Generate dummy 1-second audio array (16000 float32 samples)
        dummy_audio = np.sin(np.linspace(0, 440 * 2 * np.pi, 16000)).astype(np.float32)
        b64_audio = encode_audio_base64(dummy_audio)

        raw_json = json.dumps({
            "version": 1,
            "seq": 11,
            "session_id": "sess-test",
            "attempt_id": 2,
            "correlation_id": "tx-utt-1",
            "timestamp_ms": 1775520001000,
            "cmd": "transcribe",
            "payload": {
                "utterance_id": "1775520001000-0",
                "sequence": 1,
                "audio_data": b64_audio,
                "sample_rate": 16000,
                "channels": 1,
                "duration_ms": 1000,
                "language": "es",
                "context_prompt": "Contexto de prueba",
                "beam_size": 3,
                "timeout_ms": 10000,
            }
        })
        cmd = parse_rust_command(raw_json)
        assert cmd is not None
        assert cmd.cmd == "transcribe"
        assert isinstance(cmd.payload, TranscribePayload)
        assert cmd.payload.utterance_id == "1775520001000-0"
        assert cmd.payload.sequence == 1
        assert cmd.payload.audio_data == b64_audio

        # Validate audio decoding
        decoded = decode_audio_base64(cmd.payload.audio_data)
        assert len(decoded) == 16000
        assert decoded.dtype == np.float32
        np.testing.assert_allclose(decoded, dummy_audio, atol=1e-6)

    def test_parse_ping_and_pong_event(self):
        raw_json = json.dumps({
            "version": 1,
            "seq": 12,
            "session_id": "sess-test",
            "attempt_id": 1,
            "correlation_id": "ping-99",
            "timestamp_ms": 1775520002000,
            "cmd": "ping",
            "payload": {
                "client_monotonic_ms": 50000
            }
        })
        cmd = parse_rust_command(raw_json)
        assert cmd is not None
        assert cmd.cmd == "ping"
        assert cmd.payload.client_monotonic_ms == 50000

        # Construct Pong response event
        pong_event = WorkerEvent(
            envelope=MessageEnvelope(
                version=PROTOCOL_VERSION,
                seq=1,
                session_id=cmd.envelope.session_id,
                attempt_id=cmd.envelope.attempt_id,
                correlation_id=cmd.envelope.correlation_id,
                timestamp_ms=int(time.time() * 1000),
            ),
            event="pong",
            payload=PongPayload(
                client_monotonic_ms=50000,
                worker_monotonic_ms=50002,
                state="idle",
                queue_depth=0,
                ram_used_mb=120,
            )
        )
        wire_str = pong_event.to_wire_json()
        data = json.loads(wire_str)
        assert data["version"] == 1
        assert data["event"] == "pong"
        assert data["correlation_id"] == "ping-99"
        assert data["payload"]["state"] == "idle"
        assert data["payload"]["client_monotonic_ms"] == 50000

    def test_invalid_audio_bytes_raises(self):
        # 3 bytes is not a multiple of 4 (float32 requires 4 bytes)
        invalid_b64 = base64.b64encode(b"\x00\x01\x02").decode("ascii")
        with pytest.raises(ValueError, match="not divisible by 4"):
            decode_audio_base64(invalid_b64)

    def test_transcription_result_event_serialization(self):
        envelope = MessageEnvelope(
            version=1,
            seq=5,
            session_id="sess-result",
            attempt_id=1,
            correlation_id="tx-corr-1",
            timestamp_ms=1775520005000,
        )
        words = [
            WordTiming(word="Hola", start=0.0, end=0.4, probability=0.98),
            WordTiming(word="mundo", start=0.4, end=0.9, probability=0.99),
        ]
        segment = SegmentResult(
            id=0,
            start=0.0,
            end=0.9,
            text="Hola mundo",
            avg_logprob=-0.12,
            no_speech_prob=0.001,
            compression_ratio=1.1,
            words=words,
        )
        payload = TranscriptionResultPayload(
            utterance_id="utt-001",
            sequence=1,
            text="Hola mundo",
            language="es",
            language_probability=0.99,
            duration_sec=0.9,
            inference_sec=0.15,
            real_time_factor=0.167,
            segments=[segment],
            ram_used_mb=150,
            vram_free_mb=None,
        )
        event = WorkerEvent(envelope=envelope, event="transcription_result", payload=payload)
        json_line = event.to_wire_json()
        obj = json.loads(json_line)

        assert obj["event"] == "transcription_result"
        assert obj["payload"]["text"] == "Hola mundo"
        assert len(obj["payload"]["segments"]) == 1
        assert len(obj["payload"]["segments"][0]["words"]) == 2
        assert obj["payload"]["segments"][0]["words"][0]["word"] == "Hola"


class TestASRWorkerStateTransitions:
    """Tests for worker state machine, status emissions, and watchdog responses."""

    def test_worker_initialization_and_seq(self):
        worker = ASRWorker(session_id="test-session", attempt_id=1)
        assert worker.session_id == "test-session"
        assert worker.attempt_id == 1
        assert worker.next_seq() == 1
        assert worker.next_seq() == 2

    @patch("sys.stdout.write")
    @patch("sys.stdout.flush")
    def test_emit_status_and_event(self, mock_flush, mock_write):
        worker = ASRWorker(session_id="sess-status", attempt_id=1)
        worker.emit_status("loading", "Cargando modelo", correlation_id="req-1")

        assert mock_write.called
        assert mock_flush.called
        written_line = mock_write.call_args[0][0]
        data = json.loads(written_line)
        assert data["event"] == "status"
        assert data["payload"]["state"] == "loading"
        assert data["payload"]["text"] == "Cargando modelo"
        assert data["correlation_id"] == "req-1"

    @patch("sys.stdout.write")
    @patch("sys.stdout.flush")
    def test_instant_ping_pong_response(self, mock_flush, mock_write):
        worker = ASRWorker(session_id="sess-ping", attempt_id=1)
        worker.current_state = "idle"

        ping_cmd = parse_rust_command(json.dumps({
            "version": 1,
            "seq": 4,
            "session_id": "sess-ping",
            "attempt_id": 1,
            "correlation_id": "ping-req-42",
            "timestamp_ms": 1775520000000,
            "cmd": "ping",
            "payload": {"client_monotonic_ms": 12345}
        }))
        assert ping_cmd is not None

        # Simulate reader thread processing ping
        worker.emit_event(
            "pong",
            PongPayload(
                client_monotonic_ms=12345,
                worker_monotonic_ms=12347,
                state=worker.current_state,
                queue_depth=0,
                ram_used_mb=100,
            ),
            correlation_id=ping_cmd.envelope.correlation_id,
        )

        assert mock_write.called
        written_line = mock_write.call_args[0][0]
        data = json.loads(written_line)
        assert data["event"] == "pong"
        assert data["correlation_id"] == "ping-req-42"
        assert data["payload"]["state"] == "idle"
        assert data["payload"]["client_monotonic_ms"] == 12345

    def test_update_config_handling(self):
        worker = ASRWorker(session_id="sess-cfg", attempt_id=1)
        worker.active_config = InitPayload(model_name="tiny", language="es", beam_size=5)

        update_cmd = parse_rust_command(json.dumps({
            "version": 1,
            "seq": 5,
            "session_id": "sess-cfg",
            "attempt_id": 1,
            "cmd": "update_config",
            "payload": {
                "language": "en",
                "beam_size": 3,
                "initial_prompt": "Updated vocabulary",
            }
        }))
        assert update_cmd is not None
        worker._handle_update_config(update_cmd)

        assert worker.active_config.language == "en"
        assert worker.active_config.beam_size == 3
        assert worker.active_config.initial_prompt == "Updated vocabulary"
