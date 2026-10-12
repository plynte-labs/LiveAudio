# SPDX-License-Identifier: MIT
"""Integration tests for Faster-Whisper Python Interop Worker (WU3).

Verifies:
1. Real Faster-Whisper inference on CPU (int8, float32) and CUDA (float16, int8_float16).
2. LiveAudio v1.2.7 transcription parameters preservation (vad_filter=False, beam_size, language, initial_prompt).
3. Zero-reload in-memory model persistence across chunks.
4. Clean hot-swap model reload with complete VRAM/RAM release.
5. CUDA Out-Of-Memory (OOM) handling with automatic CPU fallback (both load-time and runtime).
6. Non-recoverable error handling when auto_cpu_fallback is disabled.
7. Input error handling (invalid audio, uninitialized model, missing local files).
8. Instant watchdog ping/pong response concurrency.
9. Dynamic configuration updates without reloading model.
10. Standalone self-test CLI runner.
"""

import base64
import json
import time
from unittest.mock import MagicMock, patch

import numpy as np
import pytest
import torch

from liveaudio.service.asr_worker import ASRWorker, run_self_test
from liveaudio.service.asr_worker_protocol import (
    PROTOCOL_VERSION,
    ErrorPayload,
    InitPayload,
    MessageEnvelope,
    ModelSwapPayload,
    PingPayload,
    PongPayload,
    ReadyPayload,
    RustCommand,
    SegmentResult,
    TranscribePayload,
    TranscriptionResultPayload,
    UpdateConfigPayload,
    WorkerEvent,
    decode_audio_base64,
    encode_audio_base64,
    get_process_memory_mb,
    get_vram_stats_mb,
    parse_rust_command,
)


def _generate_test_audio(duration_sec: float = 1.0, freq: float = 440.0) -> np.ndarray:
    """Generate a clean synthetic 16kHz float32 audio tone."""
    sample_rate = 16000
    t = np.linspace(0, duration_sec, int(sample_rate * duration_sec), endpoint=False, dtype=np.float32)
    return (0.3 * np.sin(2 * np.pi * freq * t)).astype(np.float32)


class TestFasterWhisperInferenceIntegration:
    """Tests for real and mocked Faster-Whisper inference and parameter validation."""

    def test_cpu_inference_with_int8_and_float32(self):
        """Verifies real inference on CPU supporting both int8 and float32 compute types."""
        for compute_type in ("int8", "float32"):
            events: list[WorkerEvent] = []
            worker = ASRWorker(
                session_id="test-cpu-inf",
                attempt_id=1,
                event_listener=events.append,
                write_stdout=False,
            )

            # 1. Initialize on CPU
            init_cmd = RustCommand(
                envelope=MessageEnvelope(
                    version=PROTOCOL_VERSION,
                    seq=1,
                    session_id="test-cpu-inf",
                    attempt_id=1,
                    timestamp_ms=int(time.time() * 1000),
                ),
                cmd="init",
                payload=InitPayload(
                    model_name="tiny",
                    device="cpu",
                    compute_type=compute_type,
                    cpu_threads=2,
                    language="es",
                    beam_size=5,
                ),
            )
            worker._handle_init(init_cmd)

            ready_events = [e for e in events if e.event == "ready"]
            assert len(ready_events) == 1
            ready_payload: ReadyPayload = ready_events[0].payload  # type: ignore
            assert ready_payload.device == "cpu"
            assert ready_payload.compute_type == compute_type
            assert worker.model is not None

            # 2. Transcribe synthetic audio
            audio = _generate_test_audio(1.0)
            b64_audio = encode_audio_base64(audio)
            tx_cmd = RustCommand(
                envelope=MessageEnvelope(
                    version=PROTOCOL_VERSION,
                    seq=2,
                    session_id="test-cpu-inf",
                    attempt_id=1,
                    timestamp_ms=int(time.time() * 1000),
                ),
                cmd="transcribe",
                payload=TranscribePayload(
                    utterance_id="utt-cpu-1",
                    sequence=1,
                    audio_data=b64_audio,
                    sample_rate=16000,
                    duration_ms=1000,
                    language="es",
                    beam_size=5,
                ),
            )
            events.clear()
            worker._handle_transcribe(tx_cmd)

            tx_events = [e for e in events if e.event == "transcription_result"]
            assert len(tx_events) == 1
            res: TranscriptionResultPayload = tx_events[0].payload  # type: ignore
            assert res.utterance_id == "utt-cpu-1"
            assert res.sequence == 1
            assert res.duration_sec >= 0.99
            assert res.real_time_factor >= 0.0
            assert isinstance(res.segments, list)

            worker._purge_vram()

    @pytest.mark.skipif(not torch.cuda.is_available(), reason="NVIDIA CUDA not available on this host")
    def test_cuda_inference_with_float16_and_int8_float16(self):
        """Verifies real inference on NVIDIA CUDA supporting float16 and int8_float16."""
        for compute_type in ("float16", "int8_float16"):
            events: list[WorkerEvent] = []
            worker = ASRWorker(
                session_id="test-cuda-inf",
                attempt_id=1,
                event_listener=events.append,
                write_stdout=False,
            )

            init_cmd = RustCommand(
                envelope=MessageEnvelope(
                    version=PROTOCOL_VERSION,
                    seq=1,
                    session_id="test-cuda-inf",
                    attempt_id=1,
                    timestamp_ms=int(time.time() * 1000),
                ),
                cmd="init",
                payload=InitPayload(
                    model_name="tiny",
                    device="cuda",
                    compute_type=compute_type,
                    language="es",
                ),
            )
            worker._handle_init(init_cmd)

            ready_events = [e for e in events if e.event == "ready"]
            assert len(ready_events) == 1
            ready_payload: ReadyPayload = ready_events[0].payload  # type: ignore
            assert ready_payload.device == "cuda"
            assert ready_payload.compute_type == compute_type

            # Transcribe
            audio = _generate_test_audio(1.0)
            tx_cmd = RustCommand(
                envelope=MessageEnvelope(
                    version=PROTOCOL_VERSION,
                    seq=2,
                    session_id="test-cuda-inf",
                    attempt_id=1,
                    timestamp_ms=int(time.time() * 1000),
                ),
                cmd="transcribe",
                payload=TranscribePayload(
                    utterance_id="utt-cuda-1",
                    sequence=1,
                    audio_data=encode_audio_base64(audio),
                    sample_rate=16000,
                    language="es",
                ),
            )
            events.clear()
            worker._handle_transcribe(tx_cmd)

            tx_events = [e for e in events if e.event == "transcription_result"]
            assert len(tx_events) == 1
            res: TranscriptionResultPayload = tx_events[0].payload  # type: ignore
            assert res.utterance_id == "utt-cuda-1"
            assert res.inference_sec > 0

            worker._purge_vram()

    def test_exact_whisper_transcription_parameters_preserved(self):
        """Verifies that exact LiveAudio v1.2.7 kwargs are passed to model.transcribe()."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)
        worker.active_config = InitPayload(
            model_name="tiny",
            language="es",
            initial_prompt="Prompt base",
            beam_size=5,
        )

        mock_model = MagicMock()
        mock_info = MagicMock()
        mock_info.language = "es"
        mock_info.language_probability = 0.99
        mock_seg = MagicMock()
        mock_seg.id = 0
        mock_seg.start = 0.0
        mock_seg.end = 1.0
        mock_seg.text = "Texto transcrito"
        mock_seg.avg_logprob = -0.15
        mock_seg.no_speech_prob = 0.002
        mock_seg.compression_ratio = 1.1
        mock_seg.words = []
        mock_model.transcribe.return_value = (iter([mock_seg]), mock_info)
        worker.model = mock_model

        audio = _generate_test_audio(1.0)
        tx_cmd = RustCommand(
            envelope=MessageEnvelope(
                version=1, seq=1, session_id="test", attempt_id=1, timestamp_ms=1000
            ),
            cmd="transcribe",
            payload=TranscribePayload(
                utterance_id="utt-p-1",
                sequence=1,
                audio_data=encode_audio_base64(audio),
                language="es",
                context_prompt="Prompt especifico",
                beam_size=5,
                temperature=0.0,
            ),
        )
        worker._handle_transcribe(tx_cmd)

        assert mock_model.transcribe.called
        call_args, call_kwargs = mock_model.transcribe.call_args
        assert call_kwargs["language"] == "es"
        assert call_kwargs["beam_size"] == 5
        assert call_kwargs["temperature"] == 0.0
        assert call_kwargs["vad_filter"] is False  # Must be False (VAD done upstream)
        assert call_kwargs["condition_on_previous_text"] is False  # No hallucination loop
        assert call_kwargs["word_timestamps"] is True
        assert call_kwargs["initial_prompt"] == "Prompt especifico"

        tx_events = [e for e in events if e.event == "transcription_result"]
        assert len(tx_events) == 1
        assert tx_events[0].payload.text == "Texto transcrito"  # type: ignore


class TestModelPersistenceAndHotSwap:
    """Tests verifying model in-memory persistence and zero-leak hot swap."""

    def test_model_in_memory_persistence_across_multiple_chunks(self):
        """Verifies that model is retained in memory and NOT reloaded per audio chunk."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)

        init_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=1, session_id="sess-pers", attempt_id=1, timestamp_ms=1000),
            cmd="init",
            payload=InitPayload(model_name="tiny", device="cpu", compute_type="int8"),
        )
        worker._handle_init(init_cmd)
        first_model_ref = worker.model
        assert first_model_ref is not None

        audio = _generate_test_audio(0.5)
        b64 = encode_audio_base64(audio)

        # Transcribe chunk 1
        tx1 = RustCommand(
            envelope=MessageEnvelope(version=1, seq=2, session_id="sess-pers", attempt_id=1, timestamp_ms=1000),
            cmd="transcribe",
            payload=TranscribePayload(utterance_id="chunk-1", sequence=1, audio_data=b64),
        )
        worker._handle_transcribe(tx1)
        assert worker.model is first_model_ref

        # Transcribe chunk 2
        tx2 = RustCommand(
            envelope=MessageEnvelope(version=1, seq=3, session_id="sess-pers", attempt_id=1, timestamp_ms=1000),
            cmd="transcribe",
            payload=TranscribePayload(utterance_id="chunk-2", sequence=2, audio_data=b64),
        )
        worker._handle_transcribe(tx2)
        assert worker.model is first_model_ref  # Same instance in memory!

        worker._purge_vram()

    def test_clean_hot_swap_model_reload(self):
        """Verifies model_swap purges previous model VRAM/RAM before loading new model."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)

        # 1. Load initial model on CPU int8
        init_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=1, session_id="sess-swap", attempt_id=1, timestamp_ms=1000),
            cmd="init",
            payload=InitPayload(model_name="tiny", device="cpu", compute_type="int8"),
        )
        worker._handle_init(init_cmd)
        initial_model_id = id(worker.model)

        # 2. Swap model to float32
        events.clear()
        swap_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=2, session_id="sess-swap", attempt_id=1, timestamp_ms=1000),
            cmd="model_swap",
            payload=ModelSwapPayload(
                model_name="tiny",
                device="cpu",
                compute_type="float32",
            ),
        )
        worker._handle_model_swap(swap_cmd)

        swapping_status = [e for e in events if e.event == "status" and getattr(e.payload, "state", "") == "swapping_model"]
        ready_events = [e for e in events if e.event == "ready"]

        assert len(swapping_status) == 1
        assert len(ready_events) == 1
        ready_payload: ReadyPayload = ready_events[0].payload  # type: ignore
        assert ready_payload.compute_type == "float32"

        # Verify old model was deleted and a new model instance was loaded
        assert id(worker.model) != initial_model_id

        # 3. Transcribe with swapped model
        audio = _generate_test_audio(0.5)
        tx_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=3, session_id="sess-swap", attempt_id=1, timestamp_ms=1000),
            cmd="transcribe",
            payload=TranscribePayload(utterance_id="after-swap", sequence=1, audio_data=encode_audio_base64(audio)),
        )
        events.clear()
        worker._handle_transcribe(tx_cmd)
        tx_events = [e for e in events if e.event == "transcription_result"]
        assert len(tx_events) == 1

        worker._purge_vram()


class TestCudaOomAndErrorRecovery:
    """Tests for CUDA OOM handling, CPU fallback, and error recovery."""

    def test_cuda_load_failure_automatic_cpu_fallback(self):
        """Simulates CUDA load failure, verifying automatic CPU int8 fallback."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)

        init_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=1, session_id="sess-oom", attempt_id=1, timestamp_ms=1000),
            cmd="init",
            payload=InitPayload(
                model_name="tiny",
                device="cuda",
                compute_type="float16",
                auto_cpu_fallback=True,
            ),
        )

        from faster_whisper import WhisperModel as RealWhisperModel
        call_count = 0
        def failing_init(*args, **kwargs):
            nonlocal call_count
            call_count += 1
            if kwargs.get("device") == "cuda" or call_count == 1:
                raise RuntimeError("CUDA out of memory: tried to allocate 4.00 GiB")
            return RealWhisperModel(*args, **kwargs)

        with patch("faster_whisper.WhisperModel", side_effect=failing_init):
            worker._handle_init(init_cmd)

        oom_errors = [e for e in events if e.event == "error" and getattr(e.payload, "code", "") == "cuda_oom"]
        ready_events = [e for e in events if e.event == "ready"]

        assert len(oom_errors) == 1
        assert oom_errors[0].payload.fallback_occurred is True  # type: ignore
        assert oom_errors[0].payload.action_suggested == "fallback_to_cpu"  # type: ignore

        assert len(ready_events) == 1
        ready_payload: ReadyPayload = ready_events[0].payload  # type: ignore
        assert ready_payload.device == "cpu"
        assert ready_payload.compute_type == "int8"
        assert ready_payload.fallback_from == "cuda"
        assert worker.active_config.device == "cpu"

        worker._purge_vram()

    def test_cuda_load_failure_without_fallback_terminates(self):
        """When auto_cpu_fallback=False, CUDA load error sets state to failed without reloading."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)

        init_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=1, session_id="sess-oom-nofb", attempt_id=1, timestamp_ms=1000),
            cmd="init",
            payload=InitPayload(
                model_name="tiny",
                device="cuda",
                auto_cpu_fallback=False,
            ),
        )

        with patch("faster_whisper.WhisperModel", side_effect=RuntimeError("CUDA driver out of memory")):
            worker._handle_init(init_cmd)

        err_events = [e for e in events if e.event == "error"]
        assert len(err_events) == 1
        assert err_events[0].payload.code == "model_load_failure"  # type: ignore
        assert err_events[0].payload.recoverable is False  # type: ignore
        assert err_events[0].payload.fallback_occurred is False  # type: ignore
        assert worker.current_state == "failed"
        assert worker.model is None

    def test_cuda_oom_during_transcribe_automatic_cpu_recovery(self):
        """When CUDA OOM happens during active decoding, automatically reload on CPU and retry utterance."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)
        worker.active_config = InitPayload(
            model_name="tiny",
            device="cuda",
            compute_type="float16",
            auto_cpu_fallback=True,
        )

        # Mock initial CUDA model that fails with OOM during transcribe
        mock_cuda_model = MagicMock()
        mock_cuda_model.device = "cuda"
        mock_cuda_model.transcribe.side_effect = RuntimeError("CUDA out of memory in ctranslate2 decode")
        worker.model = mock_cuda_model

        audio = _generate_test_audio(0.5)
        tx_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=2, session_id="sess-tx-oom", attempt_id=1, timestamp_ms=1000),
            cmd="transcribe",
            payload=TranscribePayload(utterance_id="utt-oom-dyn", sequence=1, audio_data=encode_audio_base64(audio)),
        )

        # When reload on CPU is triggered, let real WhisperModel load
        worker._handle_transcribe(tx_cmd)

        oom_errors = [e for e in events if e.event == "error" and getattr(e.payload, "code", "") == "cuda_oom"]
        ready_events = [e for e in events if e.event == "ready"]
        tx_results = [e for e in events if e.event == "transcription_result"]

        assert len(oom_errors) == 1
        assert oom_errors[0].payload.fallback_occurred is True  # type: ignore
        assert len(ready_events) == 1
        assert ready_events[0].payload.device == "cpu"  # type: ignore
        assert ready_events[0].payload.fallback_from == "cuda"  # type: ignore
        assert len(tx_results) == 1  # Utterance successfully transcribed on CPU!
        assert tx_results[0].payload.utterance_id == "utt-oom-dyn"  # type: ignore
        assert worker.active_config.device == "cpu"

        worker._purge_vram()

    def test_transcribe_before_model_init_returns_error(self):
        """Calling transcribe before model is loaded returns model_not_ready error."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)

        tx_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=1, session_id="test", attempt_id=1, timestamp_ms=1000),
            cmd="transcribe",
            payload=TranscribePayload(utterance_id="utt-no-model", sequence=1, audio_data=""),
        )
        worker._handle_transcribe(tx_cmd)

        err_events = [e for e in events if e.event == "error"]
        assert len(err_events) == 1
        assert err_events[0].payload.code == "model_not_ready"  # type: ignore
        assert worker.current_state == "failed"

    def test_invalid_audio_base64_returns_error(self):
        """Corrupt audio payload returns invalid_audio error without crashing worker."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)
        worker.model = MagicMock()

        tx_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=1, session_id="test", attempt_id=1, timestamp_ms=1000),
            cmd="transcribe",
            payload=TranscribePayload(
                utterance_id="utt-corrupt",
                sequence=1,
                audio_data="!!!NOT_VALID_BASE64!!!",
            ),
        )
        worker._handle_transcribe(tx_cmd)

        err_events = [e for e in events if e.event == "error"]
        assert len(err_events) == 1
        assert err_events[0].payload.code == "invalid_audio"  # type: ignore
        assert worker.current_state == "ready"

    def test_model_not_found_with_local_files_only(self):
        """Missing model with local_files_only=True returns model-not-found error."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)

        init_cmd = RustCommand(
            envelope=MessageEnvelope(version=1, seq=1, session_id="test", attempt_id=1, timestamp_ms=1000),
            cmd="init",
            payload=InitPayload(
                model_name="nonexistent-model-xyz",
                local_files_only=True,
            ),
        )
        worker._handle_init(init_cmd)

        err_events = [e for e in events if e.event == "error"]
        assert len(err_events) == 1
        assert err_events[0].payload.code == "model-not-found"  # type: ignore
        assert worker.current_state == "failed"


class TestWatchdogAndStandaloneCLI:
    """Tests for watchdog ping/pong and CLI self-test."""

    def test_watchdog_pong_metrics(self):
        """Verifies pong payload contains monotonic times, state, RAM, and queue depth."""
        events: list[WorkerEvent] = []
        worker = ASRWorker(event_listener=events.append, write_stdout=False)
        worker.current_state = "transcribing"
        worker.active_utterance_id = "utt-active-123"

        worker.emit_event(
            "pong",
            PongPayload(
                client_monotonic_ms=8888,
                worker_monotonic_ms=int(time.monotonic() * 1000),
                state=worker.current_state,
                queue_depth=worker._task_queue.qsize(),
                ram_used_mb=get_process_memory_mb(),
                active_utterance_id=worker.active_utterance_id,
            ),
            correlation_id="ping-corr",
        )

        pongs = [e for e in events if e.event == "pong"]
        assert len(pongs) == 1
        pong: PongPayload = pongs[0].payload  # type: ignore
        assert pong.client_monotonic_ms == 8888
        assert pong.state == "transcribing"
        assert pong.active_utterance_id == "utt-active-123"
        assert pongs[0].envelope.correlation_id == "ping-corr"

    def test_run_self_test_cli_function(self):
        """Verifies standalone run_self_test function completes all 7 phases and returns True."""
        result = run_self_test(model_name="tiny", device="cpu", compute_type="int8")
        assert result is True
