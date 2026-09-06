# SPDX-License-Identifier: MIT
"""Torch-free spawn targets for the ASR/audio worker processes.

On Windows multiprocessing uses 'spawn': the child re-imports the module that
holds the Process target= callable. If the GUI passed asr_consumer/audio_producer
directly, importing app.py to resolve those targets would also force the GUI
process to import their torch-heavy modules. These top-level shims live in a
module that imports nothing heavy, so the GUI can reference the targets without
pulling torch; each shim imports its real worker (and torch) only when the
child process actually runs it.
"""


def run_asr(*args):
    # REQ-6 (option a, PO 2026-09-05): pre-import heartbeat. The heavy
    # torch/faster-whisper import below can stall for minutes with zero
    # events; prove the child is alive first via put_nowait (never blocks).
    try:
        log_queue = args[2] if len(args) > 2 else None
        if log_queue is not None:
            log_queue.put_nowait({"type": "status", "key": "asr",
                                  "state": "active", "phase": "importing"})
    except Exception:
        pass
    from liveaudio.core.engine import asr_consumer
    asr_consumer(*args)


def run_audio(*args):
    from liveaudio.core.audio import audio_producer
    audio_producer(*args)
