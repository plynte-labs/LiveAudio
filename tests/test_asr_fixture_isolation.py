# SPDX-License-Identifier: MIT
"""Fake ASR consumer fixtures must never resolve real Hub models."""

import importlib
import sys
from unittest.mock import Mock, patch

import pytest


@pytest.mark.parametrize('module_name,class_name,method_name', [
    ('test_engine', 'TestAsrConsumerCanonicalTranscript', 'test_long_canonical_text_is_preserved_in_jsonl_and_ws_projection_stays_capped'),
    ('test_engine', 'TestAsrConsumerCanonicalTranscript', 'test_capture_timeline_full_vtt_text_and_safe_segment_metadata'),
    ('test_engine', 'TestAsrConsumerCanonicalTranscript', 'test_legacy_capture_without_timestamps_keeps_jsonl_but_omits_vtt_cue'),
    ('test_engine', 'TestAsrConsumerCanonicalTranscript', 'test_capture_offsets_ignore_decode_delay_and_continue_after_consumer_restart'),
    ('test_engine', 'TestAsrConsumerCanonicalTranscript', 'test_ws_queue_full_error_event_uses_capped_presentation_text'),
    ('test_resilience_asr', 'TestAsrTimeoutRecovery', 'test_capture_metadata_and_stage_timings_stay_internal_to_jsonl_and_ws_v1'),
    ('test_resilience_asr', 'TestWriterFailurePropagation', 'test_storage_write_failure_emits_transcript_free_fatal_event'),
    ('test_resilience_asr', 'TestWriterFailurePropagation', 'test_full_log_queue_writer_failure_reaches_gui_and_stops_before_alert'),
    ('test_resilience_asr', 'TestWriterFailurePropagation', 'test_backlog_transcript_notice_does_not_claim_disk_save'),
    ('test_resilience_service_backend', 'TestChildFatalPropagation', 'test_asr_storage_failure_with_full_log_queue_stops_service_run'),
    ('test_decode_deadline', 'TestDecodeDeadline', 'test_consumer_captures_configured_budget_for_each_decode'),
    ('test_vad_asr_integration', 'TestFakeVadToAsrIntegration', 'test_sixty_second_vad_phrase_is_decoded_and_persisted_once'),
])
def test_fake_asr_consumer_fixture_never_contacts_hub(monkeypatch, module_name, class_name, method_name):
    importlib.import_module('liveaudio.core.engine')
    module = importlib.import_module('tests.' + module_name)
    resolver = Mock(side_effect=AssertionError('fake ASR fixture reached real model resolver'))
    snapshot = Mock(side_effect=AssertionError('fake ASR fixture reached real Hub download'))
    monkeypatch.setattr('faster_whisper.utils.download_model', resolver)
    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    case = getattr(module, class_name)(methodName=method_name)
    with patch('sys.stdout', sys.stdout), patch('sys.stderr', sys.stderr):
        case.debug()
    resolver.assert_not_called()
    snapshot.assert_not_called()
