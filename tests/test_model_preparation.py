# SPDX-License-Identifier: MIT
"""Offline model preparation, truthful progress and recovery regressions."""

import queue
from unittest.mock import Mock

import pytest

from liveaudio.core import provisioning
from tests.test_firstuse_startup_progress import _make_supervisor


def test_cache_hit_resolves_before_device_load_without_network(monkeypatch, tmp_path):
    (tmp_path / 'model.bin').touch()
    (tmp_path / 'config.json').touch()
    (tmp_path / 'tokenizer.json').touch()
    download = Mock(return_value=str(tmp_path))
    monkeypatch.setattr('faster_whisper.utils.download_model', download)
    events = []
    result = provisioning.prepare_model('tiny', events.append, attempt=3)
    assert result == str(tmp_path)
    download.assert_called_once_with('tiny', local_files_only=True)
    assert events[0]['phase'] == 'loading'
    assert events[0]['attempt'] == 3
    assert all(event['percent'] is None for event in events)


def test_download_bytes_have_no_percent_even_when_total_grows(monkeypatch):
    from huggingface_hub.utils import LocalEntryNotFoundError
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(side_effect=LocalEntryNotFoundError('missing')))
    events = []

    def snapshot(repo_id, **kwargs):
        assert repo_id == 'mobiuslabsgmbh/faster-whisper-large-v3-turbo'
        bar = kwargs['tqdm_class'](total=10, unit='B', disable=True)
        bar.update(10)
        bar.total = 100
        bar.update(5)
        bar.close()
        return 'resolved-cache'

    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    assert provisioning.prepare_model('turbo', events.append, attempt=2) == 'resolved-cache'
    byte_events = [event for event in events if 'bytes_available' in event]
    assert byte_events
    assert byte_events[-1]['bytes_available'] == 15
    assert all(event['percent'] is None for event in events)
    assert 'resolved-cache' not in str(events)


def test_local_path_with_spaces_does_not_call_hub(monkeypatch, tmp_path):
    path = tmp_path / 'local model'
    path.mkdir()
    download = Mock(side_effect=AssertionError('unexpected hub call'))
    monkeypatch.setattr('faster_whisper.utils.download_model', download)
    assert provisioning.prepare_model(str(path), lambda event: None) == str(path)
    download.assert_not_called()


def test_cache_read_failure_is_not_treated_as_cache_miss(monkeypatch):
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(side_effect=PermissionError('private-path')))
    snapshot = Mock()
    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    with pytest.raises(PermissionError):
        provisioning.prepare_model('tiny', lambda event: None)
    snapshot.assert_not_called()


def test_supervisor_clears_percentage_when_loading_or_indeterminate():
    sup, emitter, _ = _make_supervisor()
    sup.asr_percent = 77.0
    sup.log_queue = queue.Queue()
    sup.log_queue.put(provisioning.build_progress_event('loading', None, 1, None, 'Loading'))
    sup._pump_log_queue()
    assert sup.asr_percent is None
    assert emitter.events[-1][1].get('percent') is None


def test_future_alias_uses_public_resolver_without_guessing_repo(monkeypatch):
    from huggingface_hub.utils import LocalEntryNotFoundError
    download = Mock(side_effect=[LocalEntryNotFoundError('missing'), 'future-cache'])
    monkeypatch.setattr('faster_whisper.utils.download_model', download)
    snapshot = Mock(side_effect=AssertionError('guessed repo'))
    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    events = []
    assert provisioning.prepare_model('future-alias', events.append) == 'future-cache'
    assert download.call_count == 2
    assert events[-1]['phase'] == 'downloading'
    assert events[-1]['percent'] is None
    snapshot.assert_not_called()


def test_partial_cache_download_failure_never_claims_ready(monkeypatch, tmp_path):
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(return_value=str(tmp_path)))
    monkeypatch.setattr('huggingface_hub.snapshot_download', Mock(side_effect=ConnectionError('private-url')))
    events = []
    with pytest.raises(ConnectionError):
        provisioning.prepare_model('base', events.append)
    assert all(event['phase'] != 'ready' for event in events)
    assert 'private-url' not in str(events)


def test_heartbeat_stops_after_failure_and_never_claims_download_bytes():
    events = []
    progress = provisioning.PreparationProgress(events.append, interval=0.005)
    with pytest.raises(RuntimeError):
        with progress:
            progress.report('loading', 'ASR: consultando Hugging Face')
            assert progress._stop.wait(0.02) is False
            raise RuntimeError('failed')
    assert not progress._thread.is_alive()
    assert len(events) >= 2
    assert all(event['phase'] == 'loading' and event['percent'] is None for event in events)
    assert not any('bytes_available' in event for event in events)


def test_byte_reporter_close_is_idempotent_and_throttled(monkeypatch):
    from huggingface_hub.utils import LocalEntryNotFoundError
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(side_effect=LocalEntryNotFoundError('missing')))
    emitted = []
    bars = []
    with provisioning.PreparationProgress(emitted.append) as progress:
        def snapshot(repo, **kwargs):
            bar = kwargs['tqdm_class'](unit='B', total=0)
            bars.append(bar)
            for _ in range(1000):
                bar.update(1)
            bar.close()
            bar.close()
            return 'cache'
        monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
        def forward(event):
            progress.report(event['phase'], event['text'], event.get('bytes_available'), force=event.get('force', False))
        provisioning.prepare_model('tiny', forward)
    before = len(emitted)
    bars[0].close()
    assert len(emitted) == before
    assert len([event for event in emitted if 'bytes_available' in event]) <= 3
    assert emitted[-1]['bytes_available'] == 1000


def test_engine_cuda_fallback_reuses_resolved_path(monkeypatch, tmp_path):
    from liveaudio.core import engine
    resolve = Mock(return_value='resolved-cache')
    monkeypatch.setattr(provisioning, 'prepare_model', resolve)
    model = Mock(side_effect=[RuntimeError('CUDA memory'), Mock()])
    monkeypatch.setattr(engine, 'WhisperModel', model)
    audio = queue.Queue()
    audio.put([0.0])
    audio.put(None)
    logs = queue.Queue()
    monkeypatch.setattr(engine, '_transcribe_with_timeout', lambda *args, **kwargs: ([], None))
    engine.asr_consumer(audio, queue.Queue(), logs, {'model_size': 'turbo', 'device': 'cuda', 'cpu_threads': 4}, str(tmp_path))
    resolve.assert_called_once()
    assert [call.kwargs['device'] for call in model.call_args_list] == ['cuda', 'cpu']
    assert all(call.kwargs['model_size_or_path'] == 'resolved-cache' for call in model.call_args_list)
    events = list(logs.queue)
    assert any(event.get('state') == 'ready' for event in events)
    assert not any('no encontrado' in event.get('message', '') for event in events)


def test_supervisor_forwards_bytes_and_clears_stale_percent_same_state():
    sup, emitter, _ = _make_supervisor()
    sup.asr_state = 'downloading'
    sup.asr_percent = 77.0
    event = provisioning.build_progress_event('downloading', None, 1, None, 'Download')
    event['bytes_available'] = 4096
    sup.log_queue.put(event)
    sup._pump_log_queue()
    assert sup.asr_percent is None
    assert emitter.events[-1][1]['bytes_available'] == 4096


def test_heartbeat_does_not_emit_after_stopping():
    events = []
    with provisioning.PreparationProgress(events.append) as progress:
        progress.report('loading', 'Loading')
    progress.report('downloading', 'Late bytes', 123, force=True)
    assert len(events) == 1


def test_missing_tokenizer_is_partial_cache_not_device_load(monkeypatch, tmp_path):
    (tmp_path / 'model.bin').touch()
    (tmp_path / 'config.json').touch()
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(return_value=str(tmp_path)))
    snapshot = Mock(return_value='complete-cache')
    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    assert provisioning.prepare_model('tiny', lambda event: None) == 'complete-cache'
    snapshot.assert_called_once()


@pytest.mark.parametrize('alias,repo', [
    ('large', 'Systran/faster-whisper-large-v3'),
    ('large-v3-turbo', 'mobiuslabsgmbh/faster-whisper-large-v3-turbo'),
    ('distil-large-v2', 'Systran/faster-distil-whisper-large-v2'),
    ('distil-medium.en', 'Systran/faster-distil-whisper-medium.en'),
    ('distil-small.en', 'Systran/faster-distil-whisper-small.en'),
    ('distil-large-v3', 'Systran/faster-distil-whisper-large-v3'),
    ('distil-large-v3.5', 'distil-whisper/distil-large-v3.5-ct2'),
    ('custom/model', 'custom/model'),
])
def test_aliases_preserve_current_repository_mapping(monkeypatch, alias, repo):
    from huggingface_hub.utils import LocalEntryNotFoundError
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(side_effect=LocalEntryNotFoundError('missing')))
    snapshot = Mock(return_value='cache')
    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    provisioning.prepare_model(alias, lambda event: None)
    assert snapshot.call_args.args[0] == repo


def test_legacy_hub_without_errors_module_uses_public_utils_export(monkeypatch, tmp_path):
    import sys
    from huggingface_hub.utils import LocalEntryNotFoundError
    assert issubclass(LocalEntryNotFoundError, Exception)
    for name in ('model.bin', 'config.json', 'tokenizer.json'):
        (tmp_path / name).touch()
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(return_value=str(tmp_path)))
    monkeypatch.setattr('huggingface_hub.snapshot_download', Mock(side_effect=AssertionError('unexpected network')))
    monkeypatch.setitem(sys.modules, 'huggingface_hub.errors', None)
    assert provisioning.prepare_model('tiny', lambda event: None) == str(tmp_path)


def test_legacy_file_count_bar_remains_indeterminate_without_claiming_bytes(monkeypatch):
    from huggingface_hub.utils import LocalEntryNotFoundError
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(side_effect=LocalEntryNotFoundError('missing')))
    events = []
    def snapshot(repo, **kwargs):
        with kwargs['tqdm_class'](unit='files', total=5) as bar:
            bar.update(5)
        return 'cache'
    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    provisioning.prepare_model('tiny', events.append)
    assert events[-1]['phase'] == 'downloading'
    assert all(event['percent'] is None for event in events)
    assert not any('bytes_available' in event for event in events)


@pytest.mark.parametrize("count,label", [(15, "15 B"), (1024, "1.0 KiB"), (1048576, "1.0 MiB")])
def test_small_download_counts_remain_readable(monkeypatch, count, label):
    from huggingface_hub.utils import LocalEntryNotFoundError
    monkeypatch.setattr('faster_whisper.utils.download_model', Mock(side_effect=LocalEntryNotFoundError('missing')))
    events = []
    def snapshot(repo, **kwargs):
        bar = kwargs['tqdm_class'](unit='B')
        bar.update(count)
        bar.close()
        return 'cache'
    monkeypatch.setattr('huggingface_hub.snapshot_download', snapshot)
    provisioning.prepare_model('tiny', events.append)
    assert label + ' disponibles' in events[-1]['text']
    assert events[-1]['bytes_available'] == count


def test_wait_heartbeat_shows_elapsed_phase_time_without_claiming_transfer(monkeypatch):
    clock = Mock(side_effect=[100.0, 115.0, 130.0])
    monkeypatch.setattr('time.monotonic', clock)
    events = []
    progress = provisioning.PreparationProgress(events.append)
    assert progress.interval == 15.0
    progress.report('loading', 'ASR: consultando caché local')
    progress._stop = Mock()
    progress._stop.wait.side_effect = [False, False, True]
    progress._heartbeat()
    assert events[-2]['text'].endswith('(espera: 15 s)')
    assert events[-1]['text'].endswith('(espera: 30 s)')
    assert all(e['percent'] is None and 'bytes_available' not in e for e in events)


@pytest.mark.parametrize('phase,bytes_available', [('loading', None), ('downloading', 15)])
def test_wait_clock_restarts_for_stage_and_preserves_last_bytes(monkeypatch, phase, bytes_available):
    monkeypatch.setattr('time.monotonic', Mock(side_effect=[100.0, 110.0, 125.0]))
    events = []
    progress = provisioning.PreparationProgress(events.append, attempt=4)
    progress.report('loading', 'ASR: consultando caché local')
    progress.report(phase, 'ASR: nueva etapa', bytes_available)
    progress._stop = Mock()
    progress._stop.wait.side_effect = [False, True]
    progress._heartbeat()
    assert events[-1]['text'].endswith('(espera: 15 s)')
    assert events[-1]['attempt'] == 4
    assert events[-1]['phase'] == phase
    assert events[-1]['percent'] is None
    if bytes_available is not None:
        assert events[-1]['bytes_available'] == bytes_available
