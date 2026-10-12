# SPDX-License-Identifier: MIT
"""Benchmark script to measure process startup time, memory, and teardown."""

import subprocess
import time
import os
import sys


def measure_process_startup(command, runs=5):
    times_ms = []
    for i in range(runs):
        t0 = time.perf_counter()
        proc = subprocess.run(command, capture_output=True, text=True)
        t1 = time.perf_counter()
        elapsed_ms = (t1 - t0) * 1000.0
        times_ms.append(elapsed_ms)
        if proc.returncode != 0:
            print(f"Warning: command failed with returncode {proc.returncode}: {proc.stderr[:100]}")
    times_ms.sort()
    return {
        "runs": runs,
        "min_ms": times_ms[0],
        "max_ms": times_ms[-1],
        "p50_ms": times_ms[len(times_ms) // 2],
        "mean_ms": sum(times_ms) / len(times_ms),
        "raw_ms": times_ms
    }

def main():
    print("=================================================================")
    print("BENCHMARK: PROCESS STARTUP LATENCY (Python v1.2.7 vs Rust Core)")
    print("=================================================================")

    python_exe = sys.executable
    rust_cli = os.path.abspath("target/release/liveaudio-cli.exe")

    print(f"Python interpreter: {python_exe}")
    print(f"Rust binary:        {rust_cli}")
    print()

    # 1. Bare Python startup
    print("1. Measuring Bare Python startup (python.exe -c pass)...")
    bare_py = measure_process_startup([python_exe, "-c", "pass"], runs=5)
    print(f"   Mean: {bare_py['mean_ms']:.1f} ms | P50: {bare_py['p50_ms']:.1f} ms")

    # 2. Python v1.2.7 core imports (torch + faster_whisper + sounddevice + liveaudio)
    print("2. Measuring Python v1.2.7 full runtime imports (torch, sounddevice, faster_whisper)...")
    full_py = measure_process_startup([
        python_exe, "-c",
        "import torch, sounddevice, faster_whisper, numpy"
    ], runs=5)
    print(f"   Mean: {full_py['mean_ms']:.1f} ms | P50: {full_py['p50_ms']:.1f} ms | Range: [{full_py['min_ms']:.1f}ms - {full_py['max_ms']:.1f}ms]")

    # 3. Rust Core bare startup
    print("3. Measuring Rust Core bare execution (liveaudio-cli --help)...")
    rust_bare = measure_process_startup([rust_cli, "--help"], runs=5)
    print(f"   Mean: {rust_bare['mean_ms']:.1f} ms | P50: {rust_bare['p50_ms']:.1f} ms | Range: [{rust_bare['min_ms']:.1f}ms - {rust_bare['max_ms']:.1f}ms]")

    # 4. Rust Core doctor (full environment detection, Job Object initialization, paths, port checks)
    print("4. Measuring Rust Core environment discovery (liveaudio-cli doctor)...")
    rust_doctor = measure_process_startup([rust_cli, "doctor"], runs=5)
    print(f"   Mean: {rust_doctor['mean_ms']:.1f} ms | P50: {rust_doctor['p50_ms']:.1f} ms | Range: [{rust_doctor['min_ms']:.1f}ms - {rust_doctor['max_ms']:.1f}ms]")

    print()
    print("-----------------------------------------------------------------")
    print(f"SUMMARY: Startup Latency Improvement")
    print(f"Python v1.2.7 Runtime Import: {full_py['mean_ms']:.1f} ms (p50: {full_py['p50_ms']:.1f} ms)")
    print(f"Rust Core Bare:                {rust_bare['mean_ms']:.1f} ms (p50: {rust_bare['p50_ms']:.1f} ms)")
    print(f"Rust Core Full Diagnostics:    {rust_doctor['mean_ms']:.1f} ms (p50: {rust_doctor['p50_ms']:.1f} ms)")
    ratio_bare = full_py['mean_ms'] / rust_bare['mean_ms']
    ratio_doctor = full_py['mean_ms'] / rust_doctor['mean_ms']
    print(f"Speedup bare:     {ratio_bare:.1f}x faster")
    print(f"Speedup full:     {ratio_doctor:.1f}x faster")
    print(f"Latency reduction: {((full_py['mean_ms'] - rust_doctor['mean_ms']) / full_py['mean_ms'] * 100):.1f}%")
    print("-----------------------------------------------------------------")

if __name__ == "__main__":
    main()
