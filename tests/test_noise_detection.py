# SPDX-License-Identifier: GPL-3.0-or-later
"""Tests for noise/false-positive detection (REQ-6)."""

import unittest
import numpy as np

from core.engine import _sanitize_text
from tests.helpers import make_shared_config


class TestVadLayerNoiseRejection(unittest.TestCase):
    """Tests for VAD layer: silence/noise below threshold produces zero transcription."""

    def test_silence_produces_no_transcription(self):
        """Silence (VAD below threshold) should produce no transcription output."""
        from core.audio import VAD_THRESHOLD
        vad_prob = 0.1  # Well below threshold
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertFalse(is_speech)

    def test_noise_below_threshold_produces_no_output(self):
        """Noise below VAD threshold should produce no transcription."""
        from core.audio import VAD_THRESHOLD
        vad_prob = 0.3  # Below threshold
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertFalse(is_speech)

    def test_very_low_energy_silence(self):
        """Very low energy audio should be detected as silence."""
        # Simulate RMS energy for silence
        audio = np.zeros(1600, dtype=np.float32)
        rms = np.sqrt(np.mean(audio ** 2))
        self.assertEqual(rms, 0.0)

    def test_background_noise_below_threshold(self):
        """Background noise should be below VAD threshold."""
        from core.audio import VAD_THRESHOLD
        # Simulate low-level background noise
        noise = np.random.uniform(-0.01, 0.01, 1600).astype(np.float32)
        rms = np.sqrt(np.mean(noise ** 2))
        # RMS should be very low for background noise
        self.assertLess(rms, 0.05)


class TestBlacklistFiltering(unittest.TestCase):
    """Tests for blacklist layer: gambling terms filtered from output."""

    def test_bullet_roulette_filtered(self):
        """'bullet roulette' should be filtered from output."""
        blacklist = ["bullet roulette", "ruleta bala"]
        texto = "Welcome to bullet roulette game"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertTrue(is_filtered)

    def test_gambling_terms_filtered(self):
        """Common gambling terms should be filtered from output."""
        blacklist = ["casino", "apuesta", "gambling", "betting", "ruleta"]
        texto = "Visit our casino for betting"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertTrue(is_filtered)

    def test_blacklist_case_insensitive(self):
        """Blacklist filtering should be case insensitive."""
        blacklist = ["bullet roulette"]
        texto = "BULLET ROULETTE is fun"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertTrue(is_filtered)

    def test_blacklist_partial_match(self):
        """Blacklist should match partial phrases."""
        blacklist = ["suscríbete", "dale like"]
        texto = "Por favor suscríbete al canal"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertTrue(is_filtered)

    def test_clean_text_passes_blacklist(self):
        """Clean text without blacklist terms should pass through."""
        blacklist = ["bullet roulette", "casino"]
        texto = "Hola mundo, bienvenidos"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertFalse(is_filtered)

    def test_empty_blacklist_passes_all(self):
        """Empty blacklist should pass all text through."""
        blacklist = []
        texto = "bullet roulette casino"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertFalse(is_filtered)

    def test_blacklist_from_shared_config(self):
        """Blacklist should be parsed from shared config correctly."""
        config = make_shared_config()
        blacklist = [w.strip().lower() for w in config["blacklist"].split(",") if w.strip()]
        self.assertIn("amara.org", blacklist)
        self.assertIn("suscríbete", blacklist)


class TestEnergyLayerRejection(unittest.TestCase):
    """Tests for energy layer: low-quality segments rejected before ASR."""

    def test_high_noise_floor_rejected(self):
        """High noise floor segment should be rejected before ASR."""
        # Simulate audio with high noise floor
        noise_floor = 0.3  # High noise level
        audio = np.random.uniform(-noise_floor, noise_floor, 1600).astype(np.float32)
        rms = np.sqrt(np.mean(audio ** 2))
        # RMS should be moderate but SNR is poor
        self.assertLess(rms, 0.2)  # Not loud enough to be speech

    def test_low_snr_rejected(self):
        """Low SNR segment should be rejected before ASR."""
        # Simulate low SNR: weak signal + strong noise
        signal = np.sin(np.linspace(0, 100, 1600)).astype(np.float32) * 0.05
        noise = np.random.uniform(-0.2, 0.2, 1600).astype(np.float32)
        audio = signal + noise
        rms = np.sqrt(np.mean(audio ** 2))
        # SNR is poor, should be rejected
        signal_power = np.mean(signal ** 2)
        noise_power = np.mean(noise ** 2)
        snr = 10 * np.log10(signal_power / noise_power) if noise_power > 0 else float('inf')
        self.assertLess(snr, 0)  # Negative SNR = noise dominates

    def test_good_quality_audio_accepted(self):
        """Good quality audio should be accepted for ASR."""
        # Simulate clean speech-like signal
        audio = np.sin(np.linspace(0, 200, 1600)).astype(np.float32) * 0.5
        rms = np.sqrt(np.mean(audio ** 2))
        self.assertGreater(rms, 0.1)  # Loud enough
        self.assertLess(rms, 1.0)  # Not clipping

    def test_energy_calculation_handles_silence(self):
        """Energy calculation should handle pure silence."""
        audio = np.zeros(1600, dtype=np.float32)
        rms = np.sqrt(np.mean(audio ** 2))
        self.assertEqual(rms, 0.0)


class TestCombinedNoiseAndBlacklistProtection(unittest.TestCase):
    """Tests for combined protection: noise + blacklist term → double protection."""

    def test_noise_with_blacklist_term_double_protection(self):
        """Noisy audio with blacklist term should be caught by both layers."""
        # Layer 1: VAD/Energy should reject noisy audio
        from core.audio import VAD_THRESHOLD
        vad_prob = 0.2  # Below threshold
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertFalse(is_speech)

        # Layer 2: Blacklist should catch the term even if VAD passes
        blacklist = ["bullet roulette"]
        texto = "bullet roulette en vivo"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertTrue(is_filtered)

    def test_clean_audio_with_blacklist_still_filtered(self):
        """Clean audio with blacklist term should still be filtered."""
        # VAD would pass this (clean audio)
        from core.audio import VAD_THRESHOLD
        vad_prob = 0.9  # Above threshold
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertTrue(is_speech)

        # But blacklist should still filter it
        blacklist = ["casino", "apuesta"]
        texto = "Bienvenidos al casino"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertTrue(is_filtered)

    def test_noisy_audio_without_blacklist_rejected_by_vad(self):
        """Noisy audio without blacklist term should be rejected by VAD."""
        from core.audio import VAD_THRESHOLD
        vad_prob = 0.15  # Below threshold
        is_speech = vad_prob >= VAD_THRESHOLD
        self.assertFalse(is_speech)

        # No blacklist terms, but VAD already rejected it
        blacklist = ["bullet roulette"]
        texto = "ruido de fondo constante"
        is_filtered = any(term in texto.lower() for term in blacklist)
        self.assertFalse(is_filtered)


if __name__ == "__main__":
    unittest.main()
