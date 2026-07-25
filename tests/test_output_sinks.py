# SPDX-License-Identifier: MIT
"""Tests for the independent output-sink toggles (transcript/VTT/OBS) and ws_port safety.

Covers:
  * per-utterance disk gating read live from shared_config
  * VTT cue numbering staying contiguous across an OFF period
  * no backfill of records produced while a sink was disabled
  * base-port changes surfacing the new overlay URL
"""

import json
import os
import tempfile
import types
import unittest
from unittest.mock import patch

from liveaudio.core.engine import SessionWriter, _disk_sink_decision
from liveaudio.utils.i18n import TRANSLATIONS


def _read_lines(path):
    if not os.path.exists(path):
        return []
    with open(path, "r", encoding="utf-8") as handle:
        return [line for line in handle.read().splitlines() if line.strip()]


class TestDiskSinkDecision(unittest.TestCase):
    """_disk_sink_decision resolves both gates and owns the cue counter."""

    def test_missing_keys_default_to_both_enabled(self):
        """Absent keys preserve current behavior: both sinks write, cue advances."""
        save_transcript, save_vtt, cue = _disk_sink_decision({}, 0)
        self.assertTrue(save_transcript)
        self.assertTrue(save_vtt)
        self.assertEqual(cue, 1)

    def test_vtt_disabled_does_not_consume_cue_number(self):
        """A suppressed cue must not burn a cue number."""
        _, save_vtt, cue = _disk_sink_decision({"save_vtt_enabled": False}, 7)
        self.assertFalse(save_vtt)
        self.assertEqual(cue, 7)

    def test_transcript_disabled_still_advances_cue(self):
        """JSONL off must not affect VTT numbering."""
        save_transcript, save_vtt, cue = _disk_sink_decision(
            {"save_transcript_enabled": False, "save_vtt_enabled": True}, 3
        )
        self.assertFalse(save_transcript)
        self.assertTrue(save_vtt)
        self.assertEqual(cue, 4)

    def test_both_disabled_suppresses_write_and_cue(self):
        config = {"save_transcript_enabled": False, "save_vtt_enabled": False}
        save_transcript, save_vtt, cue = _disk_sink_decision(config, 5)
        self.assertFalse(save_transcript)
        self.assertFalse(save_vtt)
        self.assertEqual(cue, 5)

    def test_cue_numbering_stays_contiguous_across_off_period(self):
        """ON, OFF, OFF, ON, ON must yield cues 1, 2, 3 — never 1, 4, 5."""
        config = {"save_vtt_enabled": True}
        cue = 0
        emitted = []
        for enabled in (True, False, False, True, True):
            config["save_vtt_enabled"] = enabled
            _, save_vtt, cue = _disk_sink_decision(config, cue)
            if save_vtt:
                emitted.append(cue)
        self.assertEqual(emitted, [1, 2, 3])


class TestSessionWriterIndependentSinks(unittest.TestCase):
    """Each artifact must be suppressible without affecting the other."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.jsonl_path = os.path.join(self.temp_dir.name, "transcript.jsonl")
        self.vtt_path = os.path.join(self.temp_dir.name, "subtitles.vtt")
        self.writer = SessionWriter(self.jsonl_path, self.vtt_path)

    def tearDown(self):
        self.writer.stop()
        self.temp_dir.cleanup()

    def _write(self, text, cue, **flags):
        self.writer.write_record({"id": text, "text": text}, "00:00:00.000", "00:00:01.000", text, cue, **flags)

    def test_transcript_only_when_vtt_disabled(self):
        self._write("only-jsonl", 1, write_transcript=True, write_vtt=False)
        self.writer.flush()
        self.assertEqual(len(_read_lines(self.jsonl_path)), 1)
        self.assertEqual(_read_lines(self.vtt_path), [])

    def test_vtt_only_when_transcript_disabled(self):
        self._write("only-vtt", 1, write_transcript=False, write_vtt=True)
        self.writer.flush()
        self.assertEqual(_read_lines(self.jsonl_path), [])
        self.assertIn("only-vtt", "\n".join(_read_lines(self.vtt_path)))

    def test_both_disabled_writes_nothing(self):
        self._write("dropped", 1, write_transcript=False, write_vtt=False)
        self.writer.flush()
        self.assertEqual(_read_lines(self.jsonl_path), [])
        self.assertEqual(_read_lines(self.vtt_path), [])

    def test_no_backfill_of_suppressed_records_after_reenable(self):
        """Text produced while OFF must never appear after the sink is re-enabled."""
        self._write("before-off", 1, write_transcript=True, write_vtt=True)
        self._write("while-off", 1, write_transcript=False, write_vtt=False)
        self._write("after-on", 2, write_transcript=True, write_vtt=True)
        self.writer.flush()

        jsonl_texts = [json.loads(line)["text"] for line in _read_lines(self.jsonl_path)]
        self.assertEqual(jsonl_texts, ["before-off", "after-on"])

        vtt_content = "\n".join(_read_lines(self.vtt_path))
        self.assertNotIn("while-off", vtt_content)
        self.assertIn("#cue:1", vtt_content)
        self.assertIn("#cue:2", vtt_content)

    def test_defaults_write_both_artifacts(self):
        """Callers that pass no flags keep the pre-existing behavior."""
        self._write("legacy", 1)
        self.writer.flush()
        self.assertEqual(len(_read_lines(self.jsonl_path)), 1)
        self.assertIn("legacy", "\n".join(_read_lines(self.vtt_path)))


class TestSinkConfigDefaults(unittest.TestCase):
    """Config keys must default to True and be coerced to bool."""

    def test_defaults_enable_both_sinks(self):
        from liveaudio.utils.config import DEFAULT_CONFIG

        self.assertIs(DEFAULT_CONFIG["save_transcript_enabled"], True)
        self.assertIs(DEFAULT_CONFIG["save_vtt_enabled"], True)

    def test_non_bool_values_are_coerced(self):
        from liveaudio.utils.config import _normalize_config

        config, updated = _normalize_config({"save_transcript_enabled": 0, "save_vtt_enabled": "yes"})
        self.assertIs(config["save_transcript_enabled"], False)
        self.assertIs(config["save_vtt_enabled"], True)
        self.assertTrue(updated)


class TestSinkTogglesAreLiveTunable(unittest.TestCase):
    """The new toggles must not force an engine restart."""

    def test_sink_toggles_do_not_require_restart(self):
        from liveaudio.app import LiveASRApp

        base = {
            "device": "cpu",
            "model_size": "small (Balance CPU)",
            "cpu_threads": 2,
            "audio_device": None,
            "silence_timeout": 0.8,
            "max_chunk_duration": 5.0,
            "vad_speech_pad_ms": 200,
            "vad_threshold": 0.5,
            "save_transcript_enabled": True,
            "save_vtt_enabled": True,
            "ws_port": 8765,
        }
        stub = types.SimpleNamespace(config_data=dict(base))
        draft = dict(base)
        draft["save_transcript_enabled"] = False
        draft["save_vtt_enabled"] = False
        needs_asr_restart, needs_audio_restart = LiveASRApp._pending_restart_flags(stub, draft)
        self.assertFalse(needs_asr_restart)
        self.assertFalse(needs_audio_restart)


class TestObsOverlayUrl(unittest.TestCase):
    """The overlay URL must pin the configured base port."""

    def test_url_embeds_configured_base_port(self):
        from liveaudio.app import _obs_overlay_url

        url = _obs_overlay_url(9000)
        self.assertTrue(url.startswith("file:///"))
        self.assertIn("subtitulos_obs.html", url)
        self.assertTrue(url.endswith("?port=9000"))
        self.assertNotIn("\\", url)


class TestWsPortChangeWarning(unittest.TestCase):
    """A base-port change strands an overlay pinned to the old port — warn loudly."""

    def _stub(self, new_port):
        return types.SimpleNamespace(config_data={"ws_port": new_port}, print_log=lambda *a, **k: None)

    def test_no_warning_when_port_unchanged(self):
        from liveaudio.app import LiveASRApp

        with patch("liveaudio.app.messagebox") as mbox:
            result = LiveASRApp._warn_if_ws_port_changed(self._stub(8765), {"ws_port": 8765})
        self.assertIsNone(result)
        mbox.showwarning.assert_not_called()

    def test_warning_names_new_url_when_port_changes(self):
        from liveaudio.app import LiveASRApp

        logged = []
        stub = self._stub(9000)
        stub.print_log = logged.append
        with patch("liveaudio.app.messagebox") as mbox:
            result = LiveASRApp._warn_if_ws_port_changed(stub, {"ws_port": 8765})

        self.assertIsNotNone(result)
        self.assertIn("?port=9000", result)
        mbox.showwarning.assert_called_once()
        warning_text = " ".join(str(arg) for arg in mbox.showwarning.call_args[0])
        self.assertIn("?port=9000", warning_text)
        self.assertTrue(any("?port=9000" in line for line in logged))


class TestSinkI18nKeys(unittest.TestCase):
    """Every new user-facing string needs both es and en entries."""

    def test_new_keys_present_in_both_languages(self):
        required = [
            "save_transcript",
            "save_vtt",
            "ws_port_label",
            "ws_port_changed_title",
            "ws_port_changed_msg",
            "log_ws_port_changed",
            "obs_guide_port_changed",
        ]
        for key in required:
            for lang in ("es", "en"):
                with self.subTest(key=key, lang=lang):
                    self.assertIn(key, TRANSLATIONS[lang])
                    self.assertTrue(str(TRANSLATIONS[lang][key]).strip())


if __name__ == "__main__":
    unittest.main()
