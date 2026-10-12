# SPDX-License-Identifier: MIT
"""Benchmark script measuring Faster-Whisper 'small' ASR inference latency,
WER (Word Error Rate), memory usage, and shutdown teardown via IPC protocol v1.
"""

import base64
import json
import os
import subprocess
import sys
import time
import numpy as np

REFERENCE_TEXT = "Hello everyone, welcome to the live stream. Today we are testing real time audio transcription."

def compute_wer(reference: str, hypothesis: str) -> float:
    """Compute standard Word Error Rate (Levenshtein distance at word level)."""
    import re
    ref_words = re.findall(r"\w+", reference.lower())
    hyp_words = re.findall(r"\w+", hypothesis.lower())

    d = np.zeros((len(ref_words) + 1, len(hyp_words) + 1), dtype=np.uint32)
    for i in range(len(ref_words) + 1):
        d[i, 0] = i
    for j in range(len(hyp_words) + 1):
        d[0, j] = j

    for i in range(1, len(ref_words) + 1):
        for j in range(1, len(hyp_words) + 1):
            if ref_words[i - 1] == hyp_words[j - 1]:
                d[i, j] = d[i - 1, j - 1]
            else:
                substitution = d[i - 1, j - 1] + 1
                insertion = d[i, j - 1] + 1
                deletion = d[i - 1, j] + 1
                d[i, j] = min(substitution, insertion, deletion)

    wer = float(d[len(ref_words), len(hyp_words)]) / float(len(ref_words))
    return wer

class IPCWorkerClient:
    def __init__(self, python_exe, worker_script):
        self.python_exe = python_exe
        self.worker_script = worker_script
        self.proc = None
        self.seq = 0

    def start(self):
        env = os.environ.copy()
        env["PYTHONUNBUFFERED"] = "1"
        env["PYTHONIOENCODING"] = "utf-8"
        t0 = time.perf_counter()
        self.proc = subprocess.Popen(
            [self.python_exe, self.worker_script],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            bufsize=1,
            env=env
        )
        # Read pre-import beacon
        line = self.proc.stdout.readline()
        t1 = time.perf_counter()
        beacon = json.loads(line)
        beacon_time_ms = (t1 - t0) * 1000.0
        return beacon, beacon_time_ms

    def send_command(self, cmd_name: str, payload: dict):
        self.seq += 1
        msg = {
            "version": 1,
            "seq": self.seq,
            "cmd": cmd_name,
            "payload": payload
        }
        self.proc.stdin.write(json.dumps(msg) + "\n")
        self.proc.stdin.flush()

    def read_event(self, target_event: str = None, timeout: float = 30.0):
        t0 = time.monotonic()
        while time.monotonic() - t0 < timeout:
            line = self.proc.stdout.readline()
            if not line:
                break
            line = line.strip()
            if not line:
                continue
            event = json.loads(line)
            if target_event is None or event.get("event") == target_event:
                return event
        raise TimeoutError(f"Timed out waiting for event {target_event}")

    def shutdown(self):
        t0 = time.perf_counter()
        self.send_command("shutdown", {"reason": "benchmark_complete"})
        # Wait for shutdown event or EOF
        try:
            evt = self.read_event("shutdown", timeout=3.0)
        except Exception:
            pass
        self.proc.stdin.close()
        self.proc.wait(timeout=3.0)
        teardown_ms = (time.perf_counter() - t0) * 1000.0
        return teardown_ms

def benchmark_device_mode(device: str, compute_type: str, audio_16k: np.ndarray, runs: int = 5):
    print(f"\n=======================================================")
    print(f"BENCHMARK: Faster-Whisper 'small' on {device.upper()} ({compute_type})")
    print(f"=======================================================")

    python_exe = sys.executable
    worker_script = os.path.abspath("liveaudio/service/asr_worker.py")
    client = IPCWorkerClient(python_exe, worker_script)

    beacon, beacon_ms = client.start()
    print(f"Worker cold spawned. Pre-import beacon received in: {beacon_ms:.2f} ms")

    # Initialize model
    print(f"Sending 'init' command: model=small, device={device}, compute_type={compute_type}...")
    t_init0 = time.perf_counter()
    client.send_command("init", {
        "model_name": "small",
        "device": device,
        "compute_type": compute_type,
        "language": "en",
        "beam_size": 1,
        "auto_cpu_fallback": True,
        "initial_prompt": None
    })
    ready_event = client.read_event("ready", timeout=60.0)
    init_time_ms = (time.perf_counter() - t_init0) * 1000.0
    payload = ready_event["payload"]
    ram_mb = payload.get("ram_used_mb", 0)
    vram_mb = payload.get("vram_total_mb", 0) - payload.get("vram_free_mb", 0) if payload.get("vram_total_mb") and payload.get("vram_free_mb") else 0
    print(f"Model READY in: {init_time_ms:.1f} ms | RAM: {ram_mb} MB | VRAM allocated: {vram_mb} MB")

    # Encode audio
    audio_bytes = audio_16k.astype(np.float32).tobytes()
    audio_b64 = base64.b64encode(audio_bytes).decode("ascii")
    audio_duration_sec = len(audio_16k) / 16000.0
    print(f"Audio sample duration: {audio_duration_sec:.2f}s ({len(audio_16k)} samples)")

    # Warmup run
    client.send_command("transcribe", {
        "utterance_id": "warmup",
        "sequence": 0,
        "audio_data": audio_b64,
        "sample_rate": 16000,
        "channels": 1,
        "duration_ms": int(audio_duration_sec * 1000),
        "language": "en",
        "beam_size": 1,
        "temperature": 0.0
    })
    warmup_result = client.read_event("transcription_result", timeout=30.0)
    print(f"Warmup finished. Transcription: '{warmup_result['payload']['text'].strip()}'")

    # Benchmark runs
    inference_times_ms = []
    transcripts = []
    ram_measurements = []
    vram_measurements = []

    for i in range(runs):
        client.send_command("transcribe", {
            "utterance_id": f"bench_{i}",
            "sequence": i + 1,
            "audio_data": audio_b64,
            "sample_rate": 16000,
            "channels": 1,
            "duration_ms": int(audio_duration_sec * 1000),
            "language": "en",
            "beam_size": 1,
            "temperature": 0.0
        })
        res = client.read_event("transcription_result", timeout=30.0)
        p = res["payload"]
        inf_ms = p["inference_sec"] * 1000.0
        inference_times_ms.append(inf_ms)
        transcripts.append(p["text"].strip())

        # Check ping pong for memory
        client.send_command("ping", {"client_monotonic_ms": int(time.time() * 1000)})
        pong = client.read_event("pong", timeout=5.0)
        pong_p = pong["payload"]
        ram_measurements.append(pong_p["ram_used_mb"])
        if pong_p.get("vram_free_mb") is not None and payload.get("vram_total_mb") is not None:
            vram_measurements.append(payload["vram_total_mb"] - pong_p["vram_free_mb"])

    # Graceful shutdown & teardown timing
    teardown_ms = client.shutdown()
    print(f"Graceful shutdown teardown time: {teardown_ms:.2f} ms")

    # Statistics
    inference_times_ms.sort()
    min_ms = inference_times_ms[0]
    max_ms = inference_times_ms[-1]
    p50_ms = inference_times_ms[len(inference_times_ms) // 2]
    mean_ms = sum(inference_times_ms) / len(inference_times_ms)
    mean_rtf = (mean_ms / 1000.0) / audio_duration_sec
    final_text = transcripts[-1]
    wer = compute_wer(REFERENCE_TEXT, final_text)

    avg_ram = sum(ram_measurements) / len(ram_measurements)
    avg_vram = (sum(vram_measurements) / len(vram_measurements)) if vram_measurements else 0.0

    print(f"\n--- Results for {device.upper()} ({compute_type}) ---")
    print(f"Inference Latency (Mean): {mean_ms:.1f} ms | P50: {p50_ms:.1f} ms | Min: {min_ms:.1f} ms | Max: {max_ms:.1f} ms")
    print(f"Real-Time Factor (RTF):  {mean_rtf:.4f}x ({1.0 / mean_rtf:.1f}x real-time speedup)")
    print(f"Transcribed Text:        '{final_text}'")
    print(f"Reference Text:          '{REFERENCE_TEXT}'")
    print(f"Word Error Rate (WER):   {wer * 100:.2f}% (Accuracy: {(1.0 - wer) * 100:.2f}%)")
    print(f"Worker Process RAM:      {avg_ram:.1f} MB")
    print(f"CUDA VRAM Allocated:     {avg_vram:.1f} MB")
    print(f"Shutdown Teardown:       {teardown_ms:.2f} ms")

    return {
        "device": device,
        "compute_type": compute_type,
        "mean_latency_ms": mean_ms,
        "p50_latency_ms": p50_ms,
        "min_latency_ms": min_ms,
        "max_latency_ms": max_ms,
        "rtf": mean_rtf,
        "realtime_speedup": 1.0 / mean_rtf,
        "transcribed_text": final_text,
        "wer": wer,
        "ram_mb": avg_ram,
        "vram_mb": avg_vram,
        "teardown_ms": teardown_ms
    }

def main():
    audio_path = "tests/reference_speech_16k.npy"
    if not os.path.exists(audio_path):
        print(f"Audio file {audio_path} not found.")
        sys.exit(1)
    audio_16k = np.load(audio_path)

    # 1. Benchmark Faster-Whisper small on CUDA float16 (RTX 3060)
    cuda_results = benchmark_device_mode("cuda", "float16", audio_16k, runs=5)

    # 2. Benchmark Faster-Whisper small on CPU int8
    cpu_results = benchmark_device_mode("cpu", "int8", audio_16k, runs=5)

    # Print Comparison
    print("\n" + "=" * 70)
    print("ASR INFERENCE LATENCY & ACCURACY COMPARISON SUMMARY")
    print("=" * 70)
    print(f"{'Metric':<28} | {'CUDA float16 (RTX 3060)':<23} | {'CPU int8 (Fallback)':<20}")
    print("-" * 70)
    print(f"{'Mean Latency':<28} | {cuda_results['mean_latency_ms']:<18.1f} ms | {cpu_results['mean_latency_ms']:<15.1f} ms")
    print(f"{'P50 Latency':<28} | {cuda_results['p50_latency_ms']:<18.1f} ms | {cpu_results['p50_latency_ms']:<15.1f} ms")
    print(f"{'Real-Time Factor (RTF)':<28} | {cuda_results['rtf']:<18.4f}x   | {cpu_results['rtf']:<15.4f}x")
    print(f"{'Speedup vs Realtime':<28} | {cuda_results['realtime_speedup']:<18.1f}x   | {cpu_results['realtime_speedup']:<15.1f}x")
    print(f"{'Word Error Rate (WER)':<28} | {cuda_results['wer'] * 100:<18.2f}%   | {cpu_results['wer'] * 100:<15.2f}%")
    print(f"{'RAM Footprint':<28} | {cuda_results['ram_mb']:<18.1f} MB  | {cpu_results['ram_mb']:<15.1f} MB")
    print(f"{'VRAM Allocated':<28} | {cuda_results['vram_mb']:<18.1f} MB  | {'0.0 MB':<20}")
    print(f"{'Shutdown Teardown':<28} | {cuda_results['teardown_ms']:<18.2f} ms | {cpu_results['teardown_ms']:<15.2f} ms")
    print("=" * 70)

    # Save results to JSON
    with open("tests/benchmark_asr_results.json", "w", encoding="utf-8") as f:
        json.dump({"cuda": cuda_results, "cpu": cpu_results}, f, indent=2)
    print("Saved benchmark results to tests/benchmark_asr_results.json")

if __name__ == "__main__":
    main()
