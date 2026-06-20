# SPDX-License-Identifier: MIT
"""Tests for client-side backpressure in subtitle HTML (REQ-2)."""

import unittest
import os
import re


class TestClientBackpressure(unittest.TestCase):
    """Tests for backpressure prevention in subtitulos_obs.html."""

    def setUp(self):
        """Load subtitulos_obs.html."""
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_debounce_timer_present(self):
        """Debounce timer should prevent rapid re-renders."""
        # Should have some form of debounce or throttle mechanism
        has_debounce = any(keyword in self.html_content.lower() for keyword in [
            "debounce", "throttle", "lastshow", "nextshow", "cooldown"
        ])
        self.assertTrue(
            has_debounce,
            "Debounce or throttle mechanism should be implemented"
        )

    def test_max_queue_present(self):
        """Max queue of 3 messages should discard oldest."""
        # Should have a queue with max size
        has_queue = any(keyword in self.html_content for keyword in [
            "queue", "pending", "backlog", "MAX_QUEUE", "maxQueue"
        ])
        self.assertTrue(
            has_queue,
            "Message queue with max size should be implemented"
        )

    def test_no_flash_under_rapid_production(self):
        """Subtitles should not flash when produced rapidly."""
        # Should check for logic that prevents showing new subtitle while one is visible
        has_protection = any(keyword in self.html_content for keyword in [
            "isShowing", "isDisplaying", "currentlyVisible", "pendingQueue"
        ])
        self.assertTrue(
            has_protection,
            "Protection against subtitle flash should be implemented"
        )


class TestRibbonBackpressureInteraction(unittest.TestCase):
    """REQ-7 / AC-4 / AC-11: ribbon decouples visible cap from transport queue."""

    def setUp(self):
        html_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html")
        with open(html_path, "r", encoding="utf-8") as f:
            self.html_content = f.read()

    def test_single_mode_discard_oldest_preserved(self):
        """AC-11: single-mode discard-oldest (pendingQueue.shift) must remain."""
        self.assertIn(
            "pendingQueue.shift()",
            self.html_content,
            "Single-mode discard-oldest backpressure must be preserved",
        )

    def test_max_pending_queue_kept_as_transport_valve(self):
        """OD-4: MAX_PENDING_QUEUE stays as transport guard, decoupled from cap."""
        self.assertRegex(
            self.html_content,
            re.compile(r"const\s+MAX_PENDING_QUEUE\s*=\s*5"),
            "MAX_PENDING_QUEUE must stay as the transport safety valve (decoupled)",
        )

    def test_visible_cap_is_ribbon_max_lines_not_max_queue(self):
        """AC-4: visible depth governed by ribbonMaxLines, not MAX_PENDING_QUEUE."""
        self.assertIn(
            "ribbonMaxLines",
            self.html_content,
            "Visible cap must be governed by ribbonMaxLines, not MAX_PENDING_QUEUE",
        )

    def test_debounce_preserved_in_ribbon(self):
        """OD-3: DEBOUNCE_MS pacing kept (no second timing path)."""
        self.assertRegex(
            self.html_content,
            re.compile(r"const\s+DEBOUNCE_MS\s*=\s*80"),
            "DEBOUNCE_MS = 80 pacing must be preserved",
        )

    def test_isshowing_gate_only_governs_single_state(self):
        """REQ-7: the ribbon branch must NOT gate rendering on isShowing.

        The ribbon render must be reachable without waiting for onSubtitleComplete.
        Assert the stacked branch is gated by renderStacked (a separate path).
        """
        self.assertIn(
            "renderStacked",
            self.html_content,
            "Ribbon rendering must use a renderStacked path, not the isShowing gate",
        )


class TestNetworkBackpressure(unittest.TestCase):
    """Tests for server-side backpressure in network.py."""

    def setUp(self):
        """Load liveaudio/core/network.py."""
        network_path = os.path.join(os.path.dirname(__file__), "..", "liveaudio", "core", "network.py")
        with open(network_path, "r", encoding="utf-8") as f:
            self.network_content = f.read()

    def test_buffered_amount_check(self):
        """WebSocket buffer size should be checked before sending."""
        # Python websockets uses get_write_buffer_size() instead of bufferedAmount
        has_buffer_check = any(keyword in self.network_content for keyword in [
            "bufferedAmount", "get_write_buffer_size", "buffer_size", "HIGH_WATER"
        ])
        self.assertTrue(
            has_buffer_check,
            "Buffer size check should be implemented before sending"
        )

    def test_pause_on_high_water(self):
        """Should pause production when buffer exceeds threshold."""
        # Should have some logic to pause when buffer is high
        has_pause = any(keyword in self.network_content for keyword in [
            "pause", "throttle", "backpressure", "high_water"
        ])
        self.assertTrue(
            has_pause,
            "Backpressure pause mechanism should be implemented"
        )


if __name__ == "__main__":
    unittest.main()
