# SPDX-License-Identifier: MIT
"""Offline integration coverage from fake capture through the ASR consumer."""

import contextlib
import json
import os
import queue
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np


class _FakeVad:
    def __init__(self, speech_frames):
        self.speech_frames = speech_frames
        self.calls = 0
        self.condition = threading.Condition()

    def __call__(self, _chunk, _sample_rate):
        with self.condition:
            self.calls += 1
            probability = 0.9 if self.calls <= self.speech_frames else 0.0
            self.condition.notify_all()
        return SimpleNamespace(item=lambda: probability)

    def wait_for_calls(self, expected):
        deadline = time.monotonic() + 10
        with self.condition:
            while self.calls < expected:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise AssertionError(f"VAD processed {self.calls} of {expected} fake frames")
                self.condition.wait(remaining)


def _produce_fake_capture(speech_frames, silence_frames=0):
    from liveaudio.core import audio as audio_module

    vad = _FakeVad(speech_frames)
    audio_queue = queue.Queue()
    log_queue = queue.Queue()
    phrase_enqueued = threading.Event()
    shutdown_ref = {}
    tracked_events = []

    def event_factory():
        event = threading.Event()
        tracked_events.append(event)
        if len(tracked_events) == 4:
            shutdown_ref["event"] = event
        return event

    class FakeInputStream:
        def __init__(self, **kwargs):
            self.callback = kwargs["callback"]
            self.active = True

        def __enter__(self):
            total_frames = speech_frames + silence_frames
            frame = np.full((512, 1), 0.25, dtype=np.float32)
            for index in range(total_frames):
                self.callback(frame, 512, None, None)
                if (index + 1) % 16 == 0 or index + 1 == total_frames:
                    vad.wait_for_calls(index + 1)
            return self

        def __exit__(self, *_args):
            self.active = False

        def close(self):
            self.active = False

    def fake_sleep(_milliseconds):
        if not phrase_enqueued.wait(10):
            raise AssertionError("VAD did not enqueue a completed phrase")
        shutdown_ref["event"].set()

    class SignallingQueue(queue.Queue):
        def put_nowait(self, item):
            super().put_nowait(item)
            if isinstance(item, dict):
                phrase_enqueued.set()

    audio_queue = SignallingQueue()

    fake_torch = SimpleNamespace(
        hub=SimpleNamespace(
            get_dir=lambda: tempfile.gettempdir(),
            load=lambda **_kwargs: (vad, None),
        ),
        inference_mode=contextlib.nullcontext,
        from_numpy=lambda chunk: chunk,
    )
    fake_sd = SimpleNamespace(
        InputStream=FakeInputStream,
        PortAudioError=RuntimeError,
        sleep=fake_sleep,
        _terminate=lambda: None,
        _initialize=lambda: None,
    )
    threading_proxy = SimpleNamespace(
        Event=event_factory,
        Lock=threading.Lock,
        Thread=threading.Thread,
    )
    config = {
        "asr_attempt": 17,
        "vad_attempt": 3,
        "max_chunk_duration": 60.0,
        "transcription_purpose": "transcription",
        "silence_timeout": 0.3,
        "vad_speech_pad_ms": 200,
        "vad_threshold": 0.5,
        "whisper_context_prompt_es": "must not be exported",
        "audio_device": {"name": "private device name"},
        "diagnostics_enabled": False,
    }

    with (
        patch.object(audio_module, "torch", fake_torch),
        patch.object(audio_module, "sd", fake_sd),
        patch.object(audio_module, "threading", threading_proxy),
    ):
        audio_module.audio_producer(audio_queue, config, log_queue)

    item = audio_queue.get_nowait()
    statuses = []
    while not log_queue.empty():
        event = log_queue.get_nowait()
        if isinstance(event, dict) and event.get("type") == "status" and event.get("key") == "vad":
            statuses.append(event)
    return item, statuses


class TestFakeVadToAsrIntegration(unittest.TestCase):
    def test_sixty_second_vad_phrase_is_decoded_and_persisted_once(self):
        from liveaudio.core.engine import MAX_SUBTITLE_CHARS, asr_consumer

        import liveaudio.core.engine as engine_module

        chunk_size = 512
        phrase_frames = 1875  # ceil(60 * 16000 / 512)
        audio_item, statuses = _produce_fake_capture(phrase_frames)
        self.assertEqual(audio_item["attempt"], 17)
        self.assertEqual(audio_item["sequence"], 1)
        self.assertGreaterEqual(len(audio_item["audio"]), (phrase_frames - 1) * chunk_size)
        self.assertLessEqual(len(audio_item["audio"]), (phrase_frames + 1 + 7) * chunk_size)
        self.assertAlmostEqual(audio_item["audio_duration_sec"], len(audio_item["audio"]) / 16000)
        self.assertEqual(audio_item["capture_config"], {
            "transcription_purpose": "transcription",
            "max_chunk_duration": 60.0,
            "silence_timeout": 0.3,
            "vad_threshold": 0.5,
            "vad_speech_pad_ms": 200,
            "sample_rate": 16000,
        })
        self.assertNotIn("whisper_context_prompt_es", audio_item["capture_config"])
        self.assertNotIn("audio_device", audio_item["capture_config"])
        self.assertTrue(any(event["text"] == "VAD: enviando frase" for event in statuses))
        self.assertFalse(any(event["text"] == "VAD: frase enviada" for event in statuses))

        segment_text = "complete " + "word " * 150
        tail_text = "final iterator segment"

        class FakeWhisper:
            def __init__(self):
                self.transcribe_calls = 0
                self.segment_yields = 0
                self.audio_samples = None

            def transcribe(self, audio, **_kwargs):
                self.transcribe_calls += 1
                self.audio_samples = len(audio)

                def segments():
                    self.segment_yields += 1
                    yield SimpleNamespace(text=segment_text, no_speech_prob=0.0)
                    self.segment_yields += 1
                    yield SimpleNamespace(text=tail_text, no_speech_prob=0.0)

                return segments(), SimpleNamespace()

        fake_model = FakeWhisper()
        text_queue = queue.Queue()
        log_queue = queue.Queue()
        with tempfile.TemporaryDirectory() as session_dir:
            audio_queue = queue.Queue()
            audio_queue.put(audio_item)
            audio_queue.put(None)
            shared_config = {
                "asr_attempt": 17,
                "model_size": "tiny",
                "device": "cpu",
                "cpu_threads": 1,
                "blacklist": "",
                "subtitle_backlog_policy": "send_all",
                "subtitle_style": "default",
                "save_transcript_enabled": True,
                "save_vtt_enabled": True,
                "obs_enabled": True,
                "diagnostics_enabled": False,
                "session_started_monotonic": audio_item["capture_started_monotonic"] - 5.0,
            }
            with patch.object(engine_module, "WhisperModel", return_value=fake_model):
                asr_consumer(audio_queue, text_queue, log_queue, shared_config, session_dir)

            with open(os.path.join(session_dir, "transcript.jsonl"), encoding="utf-8") as handle:
                records = [json.loads(line) for line in handle]
            with open(os.path.join(session_dir, "subtitles.vtt"), encoding="utf-8") as handle:
                vtt = handle.read()
            payloads = []
            while not text_queue.empty():
                payloads.append(text_queue.get_nowait())
            transcript_events = []
            while not log_queue.empty():
                event = log_queue.get_nowait()
                if event.get("type") == "transcript":
                    transcript_events.append(event)

        expected_text = " ".join(f"{segment_text} {tail_text}".split())
        self.assertEqual(fake_model.transcribe_calls, 1)
        self.assertEqual(fake_model.segment_yields, 2)
        self.assertEqual(fake_model.audio_samples, len(audio_item["audio"]))
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["text"], expected_text)
        from liveaudio.core.engine import _format_vtt_time
        expected_timing = (
            f"{_format_vtt_time(5.0)} --> "
            f"{_format_vtt_time(5.0 + records[0]['audio_duration_sec'])}"
        )
        self.assertIn(expected_timing, vtt)
        self.assertIn(expected_text, vtt)
        self.assertEqual(len(payloads), 1)
        self.assertEqual(payloads[0]["_telemetry"]["attempt"], 17)
        self.assertEqual(payloads[0]["text"], expected_text[:MAX_SUBTITLE_CHARS] + "...")
        self.assertEqual(len(transcript_events), 1)
        self.assertEqual(transcript_events[0]["text"], expected_text[:MAX_SUBTITLE_CHARS] + "...")

    def test_vad_closes_before_duration_cap_when_silence_arrives(self):
        from liveaudio.core.audio import CHUNK_SIZE, vad_pre_buffer_chunks

        audio_item, statuses = _produce_fake_capture(speech_frames=4, silence_frames=10)

        self.assertEqual(len(audio_item["audio"]), 14 * CHUNK_SIZE)
        self.assertLess(len(audio_item["audio"]), 60 * 16000)
        self.assertEqual(audio_item["attempt"], 17)
        self.assertEqual(vad_pre_buffer_chunks(200), 7)
        self.assertTrue(any(event["text"] == "VAD: frase enviada" for event in statuses))
        self.assertFalse(any(event["text"] == "VAD: enviando frase" for event in statuses))


if __name__ == "__main__":
    unittest.main()
