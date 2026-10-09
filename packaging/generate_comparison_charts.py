# SPDX-License-Identifier: MIT
"""
Generates modern SVG comparison charts between LiveAudio Python v1.2.7 and Rust Core + Tauri 2.
Zero external dependencies (uses standard library only).
"""

import os
from pathlib import Path

CHARTS_DIR = Path("docs/migration/charts")
CHARTS_DIR.mkdir(parents=True, exist_ok=True)

def generate_bar_chart_svg(
    title: str,
    subtitle: str,
    labels: list[str],
    values: list[float],
    unit: str,
    lower_is_better: bool = True,
    colors: list[str] = None,
) -> str:
    width = 720
    height = 360
    margin_left = 180
    margin_right = 100
    margin_top = 80
    margin_bottom = 40
    plot_width = width - margin_left - margin_right
    bar_height = 42
    gap = 28

    max_val = max(values) if values else 1.0

    svg_lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">',
        '  <style>',
        '    .title { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 18px; font-weight: 700; fill: #f1f5f9; }',
        '    .subtitle { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 13px; fill: #94a3b8; }',
        '    .label { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 14px; font-weight: 600; fill: #cbd5e1; }',
        '    .val { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 14px; font-weight: 700; fill: #ffffff; }',
        '    .delta { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; font-size: 12px; font-weight: 700; fill: #10b981; }',
        '  </style>',
        f'  <rect width="{width}" height="{height}" rx="12" fill="#0b0f11" stroke="#2b3d44" stroke-width="1.5" />',
        f'  <text x="24" y="34" class="title">{title}</text>',
        f'  <text x="24" y="56" class="subtitle">{subtitle} ({("Menor valor es mejor" if lower_is_better else "Mayor valor es mejor")})</text>',
    ]

    for i, (label, val) in enumerate(zip(labels, values)):
        y = margin_top + i * (bar_height + gap)
        w = max(6, int((val / max_val) * plot_width))
        color = colors[i] if colors and i < len(colors) else ("#ef4444" if i == 0 else "#10b981")

        # Label
        svg_lines.append(f'  <text x="{margin_left - 15}" y="{y + 26}" text-anchor="end" class="label">{label}</text>')
        # Bar track
        svg_lines.append(f'  <rect x="{margin_left}" y="{y}" width="{plot_width}" height="{bar_height}" rx="6" fill="#1b262a" />')
        # Value bar
        svg_lines.append(f'  <rect x="{margin_left}" y="{y}" width="{w}" height="{bar_height}" rx="6" fill="{color}" />')
        # Value text
        val_str = f"{val:,.1f} {unit}" if isinstance(val, float) else f"{val:,} {unit}"
        svg_lines.append(f'  <text x="{margin_left + w + 12}" y="{y + 26}" class="val">{val_str}</text>')

    # Calculate delta
    if len(values) >= 2 and values[0] > 0 and values[1] > 0:
        if lower_is_better:
            factor = values[0] / values[1]
            reduction = ((values[0] - values[1]) / values[0]) * 100
            summary = f"🚀 Mejora: {factor:.1f}x más rápido ({reduction:.1f}% de reducción)"
        else:
            factor = values[1] / values[0]
            summary = f"🚀 Mejora: {factor:.1f}x mayor rendimiento"
        svg_lines.append(f'  <text x="{margin_left}" y="{height - 18}" class="delta">{summary}</text>')

    svg_lines.append('</svg>')
    return '\n'.join(svg_lines)

def main():
    charts = [
        {
            "filename": "startup_latency.svg",
            "title": "Tiempo de Inicio en Frío (Cold Startup)",
            "subtitle": "Latencia desde invocación hasta disponibilidad operativa",
            "labels": ["Python v1.2.7", "Rust Core (CLI)", "Rust Core (Bare)"],
            "values": [2811.6, 16.9, 8.0],
            "unit": "ms",
            "lower_is_better": True,
            "colors": ["#ef4444", "#06b6d4", "#10b981"],
        },
        {
            "filename": "memory_footprint.svg",
            "title": "Consumo de Memoria RAM en Reposo (Idle)",
            "subtitle": "Huella de memoria residente sin inferencia activa",
            "labels": ["Python v1.2.7 (Multi-Proc)", "Rust Core + Idle Worker", "Rust Core Aislado"],
            "values": [958.6, 73.0, 28.0],
            "unit": "MB",
            "lower_is_better": True,
            "colors": ["#ef4444", "#38bdf8", "#10b981"],
        },
        {
            "filename": "vad_latency.svg",
            "title": "Latencia de Detección de Voz (VAD por frame de 32ms)",
            "subtitle": "Silero VAD: PyTorch vs ONNX Runtime Rust",
            "labels": ["Python (PyTorch VAD)", "Rust ONNX (Debug)", "Rust ONNX (Release)"],
            "values": [0.358, 0.157, 0.114],
            "unit": "ms",
            "lower_is_better": True,
            "colors": ["#ef4444", "#f59e0b", "#10b981"],
        },
        {
            "filename": "shutdown_latency.svg",
            "title": "Latencia de Cierre Limpio (Shutdown & Teardown)",
            "subtitle": "Terminación de subprocesos y liberación de recursos (0 zombies)",
            "labels": ["Python v1.2.7", "Rust (Win32 Job Object)"],
            "values": [3401.0, 6.6],
            "unit": "ms",
            "lower_is_better": True,
            "colors": ["#ef4444", "#10b981"],
        },
        {
            "filename": "asr_throughput.svg",
            "title": "Inferencia Faster-Whisper (Small en RTX 3060)",
            "subtitle": "Velocidad respecto al tiempo real (Real-Time Speedup)",
            "labels": ["Python Baseline", "Rust CPU Fallback (int8)", "Rust CUDA (float16)"],
            "values": [1.0, 2.99, 25.74],
            "unit": "x RT",
            "lower_is_better": False,
            "colors": ["#64748b", "#38bdf8", "#10b981"],
        },
    ]

    for c in charts:
        svg_content = generate_bar_chart_svg(
            title=c["title"],
            subtitle=c["subtitle"],
            labels=c["labels"],
            values=c["values"],
            unit=c["unit"],
            lower_is_better=c["lower_is_better"],
            colors=c.get("colors"),
        )
        out_path = CHARTS_DIR / c["filename"]
        out_path.write_text(svg_content, encoding="utf-8")
        print(f"Generated chart: {out_path}")

if __name__ == "__main__":
    main()
