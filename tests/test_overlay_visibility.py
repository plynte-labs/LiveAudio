# SPDX-License-Identifier: MIT
"""Runtime coverage for overlay behavior across document visibility changes.

OBS CEF -- like any browser -- SUSPENDS requestAnimationFrame callbacks while
the document is hidden (an inactive scene, a backgrounded tab), independently of
the "Shutdown source when not visible" setting, while setTimeout keeps firing,
merely throttled. Two defects follow from that asymmetry and are pinned here:

  revealWithoutRaf -> the reveal of the transition-based styles (default, neon,
                      minimal, bold) must NOT be gated behind
                      requestAnimationFrame; otherwise a subtitle enters the DOM
                      and never becomes visible while the scene is inactive,
                      while its hide/cleanup timers keep running.
  visibilityReset  -> on becoming visible the overlay must HARD RESET: nothing
                      produced while hidden may survive or flash, and a subtitle
                      arriving afterwards must render normally. Reproduces the
                      user report: "every time I switch tab it shows one
                      subtitle and that's it."

These execute the real <script> of liveaudio/assets/subtitulos_obs.html through
tests/ribbon_harness.js, whose rAF shim is suspended while document.hidden is
true and flushed on unhide, as a real browser does.
"""

import json
import shutil
import subprocess
import unittest
from pathlib import Path


@unittest.skipUnless(shutil.which("node"), "Node.js is required for overlay runtime tests")
class TestOverlayVisibility(unittest.TestCase):
    def run_scenario(self, name):
        harness = Path(__file__).with_name("ribbon_harness.js")
        result = subprocess.run(
            ["node", str(harness), name], capture_output=True, text=True, check=True
        )
        return json.loads(result.stdout)

    def test_reveal_does_not_depend_on_request_animation_frame(self):
        """Every transition-based style reveals with rAF permanently suspended."""
        result = self.run_scenario("revealWithoutRaf")
        for style, revealed in result["revealed"].items():
            self.assertTrue(
                revealed,
                f"style '{style}' must gain .visible while the document is hidden; "
                "an rAF-gated reveal never runs inside an inactive OBS scene",
            )
        self.assertEqual(
            result["rafPending"],
            0,
            "the reveal path must not enqueue any requestAnimationFrame callback",
        )

    def test_becoming_visible_drops_everything_produced_while_hidden(self):
        """Unhiding hard-resets: no stale box survives and none of them flashes."""
        result = self.run_scenario("visibilityReset")
        self.assertTrue(
            result["liveWhileHidden"],
            "precondition: the overlay must still be rendering while hidden",
        )
        self.assertEqual(
            result["afterUnhide"],
            [],
            "no subtitle produced while the scene was inactive may survive the "
            f"switch back to visible; found {result['afterUnhide']}",
        )
        self.assertTrue(
            result["staleDroppedSilently"],
            "stale boxes must be detached WITHOUT being revealed or run through "
            "the .hiding exit animation, so nothing flashes on resume",
        )
        self.assertFalse(
            result["ribbonAfterUnhide"],
            "the ribbon-active layout must be reset along with the state machine",
        )

    def test_subtitle_arriving_after_unhide_renders_normally(self):
        """The post-resume line must render -- not be swallowed by stale state."""
        result = self.run_scenario("visibilityReset")
        self.assertEqual(
            result["freshTexts"],
            ["fresh"],
            "a subtitle arriving after the overlay became visible must render "
            "exactly one box with its own text; leftover pendingQueue/isShowing "
            f"state would swallow it. Found {result['freshTexts']}",
        )
        self.assertTrue(
            result["freshVisible"],
            "the post-resume subtitle must actually become visible",
        )


if __name__ == "__main__":
    unittest.main()
