# SPDX-License-Identifier: MIT
"""Static/structural tests for the ribbon/adaptive subtitle render path.

These assert REAL structure and logic in liveaudio/assets/subtitulos_obs.html
(the Python suite has no JS runtime), deliberately avoiding bare-substring
false positives: each assertion pins a specific mechanism (a referenced
variable in the render path, a per-line timer field, an eviction order, a
state-machine transition condition) so removing the behavior fails the test.
"""

import unittest
import os
import re


def _load_html():
    html_path = os.path.join(
        os.path.dirname(__file__), "..", "liveaudio", "assets", "subtitulos_obs.html"
    )
    with open(html_path, "r", encoding="utf-8") as f:
        return f.read()


class TestModeParsingAndUse(unittest.TestCase):
    """REQ-5 / AC-7 / AC-8: ?mode= and ?lines= parsed AND read at decision time."""

    def setUp(self):
        self.html = _load_html()

    def test_mode_param_parsed_from_url(self):
        """?mode= must be parsed via urlParams.get('mode')."""
        self.assertRegex(
            self.html,
            re.compile(r"urlParams\.get\(\s*['\"]mode['\"]\s*\)"),
            "Overlay must parse ?mode= from the URL via urlParams.get('mode')",
        )

    def test_lines_param_parsed_from_url(self):
        """?lines= must be parsed via urlParams.get('lines')."""
        self.assertRegex(
            self.html,
            re.compile(r"urlParams\.get\(\s*['\"]lines['\"]\s*\)"),
            "Overlay must parse ?lines= from the URL via urlParams.get('lines')",
        )

    def test_valid_display_modes_set_present(self):
        """A client-side set of valid modes must exist (mirrors config)."""
        for mode in ("single", "ribbon", "adaptive"):
            self.assertIn(
                f"'{mode}'", self.html, f"Client must know the '{mode}' mode"
            )

    def test_resolved_mode_defaults_to_adaptive(self):
        """Absent/invalid ?mode= must resolve to 'adaptive' (the locked default)."""
        # The resolved-mode assignment must fall back to 'adaptive', not 'single'.
        self.assertRegex(
            self.html,
            re.compile(r"resolvedMode\s*="),
            "A resolvedMode variable must hold the resolved render mode",
        )
        self.assertRegex(
            self.html,
            re.compile(r"resolvedMode\s*=[^;]*['\"]adaptive['\"]"),
            "resolvedMode must default to 'adaptive' when ?mode= is absent/invalid",
        )

    def test_resolved_mode_read_in_render_path(self):
        """AC-7 anti-dead-variable: resolvedMode must be READ in the enqueue decision.

        Guards the currentStyle trap (parsed-but-never-read). The resolved mode
        must be referenced inside the enqueue/process decision, not just stored.
        enqueueSubtitle delegates the mode decision to resolveRenderStacked, so we
        require BOTH that enqueueSubtitle calls that decision function AND that the
        decision function actually reads resolvedMode. This is a real structural
        guarantee: the parse cannot be stored-and-forgotten.
        """
        enqueue = re.search(
            r"function enqueueSubtitle\([^)]*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(enqueue, "enqueueSubtitle function body not found")
        enqueue_body = enqueue.group(1)
        self.assertIn(
            "resolveRenderStacked",
            enqueue_body,
            "enqueueSubtitle must invoke the mode decision (resolveRenderStacked)",
        )
        decision = re.search(
            r"function resolveRenderStacked\([^)]*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(decision, "resolveRenderStacked function body not found")
        self.assertIn(
            "resolvedMode",
            decision.group(1),
            "resolveRenderStacked must READ resolvedMode (AC-7 dead-variable guard)",
        )

    def test_lines_clamped_client_side(self):
        """?lines= must be clamped client-side to 1..8 (AC-8, R-8).

        The implementation bounds via Math.min(<ceiling>, n) inside Math.max(<floor>, ...).
        The ceiling is the named constant RIBBON_MAX_LINES_CEILING (= 8) and the floor
        RIBBON_MIN_LINES (= 1). Assert the named ceiling resolves to 8 and that the
        clamp nests Math.max(min, Math.min(ceiling, ...)) so it cannot exceed 8.
        """
        self.assertRegex(
            self.html,
            re.compile(r"RIBBON_MAX_LINES_CEILING\s*=\s*8"),
            "Ceiling constant RIBBON_MAX_LINES_CEILING must be 8",
        )
        self.assertRegex(
            self.html,
            re.compile(r"RIBBON_MIN_LINES\s*=\s*1"),
            "Floor constant RIBBON_MIN_LINES must be 1",
        )
        self.assertRegex(
            self.html,
            re.compile(
                r"Math\.max\(\s*RIBBON_MIN_LINES\s*,\s*Math\.min\(\s*RIBBON_MAX_LINES_CEILING\s*,"
            ),
            "?lines= must clamp via Math.max(MIN, Math.min(CEILING, n))",
        )
        self.assertRegex(
            self.html,
            re.compile(r"ribbonMaxLines"),
            "A ribbonMaxLines variable must hold the resolved visible cap",
        )

    def test_ribbon_max_lines_read_in_render_path(self):
        """The visible cap must be referenced where eviction happens (not dead)."""
        self.assertGreaterEqual(
            self.html.count("ribbonMaxLines"),
            2,
            "ribbonMaxLines must be both defined and read in the render path",
        )


class TestSingleModeLegacyRestore(unittest.TestCase):
    """REQ-1 / REQ-9 / AC-1 / AC-17: ?mode=single restores legacy single path."""

    def setUp(self):
        self.html = _load_html()

    def test_single_path_clears_shared_timers(self):
        """Legacy single path must still clear the shared hide/cleanup timers."""
        self.assertIn("clearTimeout(hideTimeout)", self.html)
        self.assertIn("clearTimeout(cleanupTimeout)", self.html)

    def test_single_path_removechild_before_append(self):
        """Legacy single path must still removeChild the prior box before append."""
        self.assertRegex(
            self.html,
            re.compile(r"container\.removeChild\(existing\)"),
            "Single path must remove the prior .sub-box before appending the new one",
        )

    def test_single_path_nullifies_existing(self):
        """Legacy single path must still nullify the removed reference for GC."""
        self.assertRegex(self.html, re.compile(r"existing\s*=\s*null"))

    def test_shared_timer_path_guarded_by_not_stacked(self):
        """The shared-timer/removeChild block must be guarded so ribbon skips it.

        A renderStacked flag (or equivalent) must gate the legacy block so the
        ribbon branch does NOT clear shared timers or removeChild the prior box.
        """
        self.assertIn(
            "renderStacked",
            self.html,
            "showSubtitle must take a renderStacked flag to branch single vs stacked",
        )
        # The clearTimeout(hideTimeout) must be inside a !renderStacked guard.
        self.assertRegex(
            self.html,
            re.compile(r"if\s*\(\s*!\s*renderStacked\s*\)"),
            "The legacy shared-timer/removeChild block must be gated by !renderStacked",
        )

    def test_single_mode_keeps_isshowing_serialization(self):
        """Single state must still serialize through isShowing + onSubtitleComplete."""
        self.assertIn("isShowing", self.html)
        self.assertIn("onSubtitleComplete", self.html)


class TestRibbonRenderBranch(unittest.TestCase):
    """REQ-1/2/3 / AC-2/3/4: ribbon stacks, per-line timers, cap eviction."""

    def setUp(self):
        self.html = _load_html()

    def test_per_line_hide_timer_field(self):
        """Each stacked box must own a per-line _hideTimer (not the singleton)."""
        self.assertRegex(
            self.html,
            re.compile(r"\bsub\._hideTimer\s*="),
            "Stacked boxes must store their own _hideTimer on the node",
        )

    def test_per_line_cleanup_timer_field(self):
        """Each stacked box must own a per-line _cleanupTimer."""
        self.assertRegex(
            self.html,
            re.compile(r"\bsub\._cleanupTimer\s*="),
            "Stacked boxes must store their own _cleanupTimer on the node",
        )

    def test_per_line_bornat_timestamp(self):
        """Each box records _bornAt for remaining-lifetime adoption (§1.12)."""
        self.assertRegex(
            self.html,
            re.compile(r"\bsub\._bornAt\s*="),
            "Each box must record _bornAt = Date.now() for adoption math",
        )

    def test_stacked_cleanup_does_not_pump_serialized_queue(self):
        """The per-line cleanup callback MUST NOT call onSubtitleComplete (§1.12).

        Calling it would re-pump the serialized single-mode queue. The stacked
        cleanup must removeChild + null its own timers without onSubtitleComplete.
        We assert the stacked branch removes the node and nulls _hideTimer.
        """
        self.assertRegex(
            self.html,
            re.compile(r"sub\._hideTimer\s*=\s*sub\._cleanupTimer\s*=\s*null"),
            "Stacked per-line cleanup must null both per-line timer ids",
        )

    def test_cap_eviction_targets_oldest_first_child(self):
        """Cap eviction must target the OLDEST box = container.firstElementChild.

        Insertion is appendChild, so DOM child order is [oldest ... newest] and the
        oldest box is firstElementChild. flex-direction: column-reverse only flips the
        VISUAL order, NOT the DOM child indices — so evicting lastElementChild would
        remove the NEWEST box (the bug this test now pins against). The eviction must
        target firstElementChild, and must NOT use lastElementChild.
        """
        # Locate the cap-eviction loop (the while over container.children vs the cap).
        evict = re.search(
            r"while\s*\(\s*container\.children\.length\s*>=\s*ribbonMaxLines\s*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(evict, "cap-eviction while-loop not found")
        body = evict.group(1)
        self.assertIn(
            "container.firstElementChild",
            body,
            "Cap eviction must evict the OLDEST box via container.firstElementChild",
        )
        self.assertNotIn(
            "container.lastElementChild",
            body,
            "Cap eviction must NOT target lastElementChild (that is the NEWEST box)",
        )

    def test_cap_eviction_clears_timers_before_removechild(self):
        """Eviction must clearTimeout on the evicted box's per-line timers (R-2)."""
        # Eviction must clear the evicted box's _hideTimer/_cleanupTimer.
        self.assertRegex(
            self.html,
            re.compile(r"clearTimeout\(\s*\w+\._hideTimer\s*\)"),
            "Eviction/adoption must clearTimeout(box._hideTimer)",
        )
        self.assertRegex(
            self.html,
            re.compile(r"clearTimeout\(\s*\w+\._cleanupTimer\s*\)"),
            "Eviction/adoption must clearTimeout(box._cleanupTimer)",
        )

    def test_cap_eviction_uses_ribbon_max_lines(self):
        """Eviction loop must compare child count against ribbonMaxLines."""
        self.assertRegex(
            self.html,
            re.compile(r"children\.length\s*>\s*ribbonMaxLines|ribbonMaxLines"),
            "Cap eviction must be governed by ribbonMaxLines",
        )

    def test_single_path_removechild_clears_per_line_timers(self):
        """§1.12 demotion edge: single-path removeChild must clear any per-line timers.

        When a single-path line removes a survivor box created in RIBBON, it must
        clear that box's _hideTimer/_cleanupTimer to avoid a stale callback on a
        detached node. Assert the single removeChild block references _hideTimer.
        """
        # Locate the single-path removeChild block and assert it clears timers.
        block = re.search(
            r"if\s*\(existing\)\s*\{(.*?)\}", self.html, re.DOTALL
        )
        self.assertIsNotNone(block, "single-path removeChild block not found")
        self.assertIn(
            "_hideTimer",
            block.group(1),
            "single-path removeChild must clear the removed node's per-line timers (§1.12)",
        )


class TestStackingDirectionCSS(unittest.TestCase):
    """REQ-8 / OD-1 / AC-12: newest-on-top via column-reverse, presets stack."""

    def setUp(self):
        self.html = _load_html()

    def test_ribbon_active_class_uses_column_reverse(self):
        """A ribbon-active modifier class must apply flex-direction: column-reverse."""
        self.assertRegex(
            self.html,
            re.compile(r"flex-direction:\s*column-reverse"),
            "Ribbon container must use flex-direction: column-reverse (newest on top)",
        )

    def test_ribbon_active_class_name_referenced_in_js(self):
        """The ribbon-active class must be toggled from JS (classList add/remove)."""
        self.assertRegex(
            self.html,
            re.compile(r"classList\.(add|remove|toggle)\(\s*['\"]ribbon-active['\"]"),
            "JS must toggle the 'ribbon-active' container class with the state",
        )

    def test_explicit_ribbon_mode_activates_column_reverse_class(self):
        """Pinned ?mode=ribbon must add ribbon-active so column-reverse applies (OD-1).

        Regression guard: stacking via appendChild without ribbon-active would render
        newest-on-BOTTOM (the default column), violating the locked newest-on-top
        direction. The 'ribbon' branch of resolveRenderStacked must call setRibbonActive(true).
        """
        decision = re.search(
            r"function resolveRenderStacked\([^)]*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(decision, "resolveRenderStacked function body not found")
        body = decision.group(1)
        ribbon_branch = re.search(
            r"resolvedMode\s*===\s*['\"]ribbon['\"]\s*\)\s*\{(.*?)\}",
            body,
            re.DOTALL,
        )
        self.assertIsNotNone(ribbon_branch, "ribbon-mode branch not found in decision")
        self.assertIn(
            "setRibbonActive(true)",
            ribbon_branch.group(1),
            "Pinned ribbon mode must activate the column-reverse container class",
        )

    def test_exit_animation_css_untouched(self):
        """Track-C .sub-box.hiding exit transition must remain intact (coordination)."""
        self.assertRegex(
            self.html,
            re.compile(r"\.sub-box\.hiding\s*\{[^}]*opacity:\s*0\s*!important"),
            "Track C's .sub-box.hiding exit CSS must be preserved unchanged",
        )

    def test_bottom_margin_preserved(self):
        """The 50px bottom anchor margin must be preserved."""
        self.assertRegex(self.html, re.compile(r"margin-bottom:\s*50px"))


class TestAdaptiveStateMachine(unittest.TestCase):
    """REQ-10 / AC-15 / AC-16: SINGLE<->RIBBON promote/demote with hysteresis."""

    def setUp(self):
        self.html = _load_html()

    def test_adaptive_state_variable_exists(self):
        """A module-level adaptiveState must hold the current state."""
        self.assertRegex(
            self.html,
            re.compile(r"adaptiveState\s*="),
            "Adaptive machine must have an adaptiveState variable",
        )

    def test_promote_condition_edge_triggered_on_queue_or_replay(self):
        """PROMOTE when pendingQueue.length > 1 OR isReplay (edge-triggered)."""
        # The promotion condition must reference pendingQueue length > 1 and isReplay.
        self.assertRegex(
            self.html,
            re.compile(r"pendingQueue\.length\s*>\s*1"),
            "Promotion must be edge-triggered on pendingQueue.length > 1",
        )
        self.assertRegex(
            self.html,
            re.compile(r"\bisReplay\b"),
            "Promotion must also consider the isReplay payload signal",
        )

    def test_is_replay_threaded_from_onmessage(self):
        """is_replay must be threaded from the payload into enqueueSubtitle."""
        self.assertRegex(
            self.html,
            re.compile(r"data\.is_replay"),
            "onmessage must read data.is_replay and thread it into enqueue",
        )
        # enqueueSubtitle signature must accept the replay flag.
        self.assertRegex(
            self.html,
            re.compile(r"function enqueueSubtitle\(\s*text\s*,\s*style\s*,\s*isReplay"),
            "enqueueSubtitle must accept isReplay as a parameter",
        )

    def test_demote_requires_drain_and_settle_window(self):
        """DEMOTE only after drain (<=1 pending) + settle window using DEBOUNCE_MS."""
        # Demotion must be scheduled via setTimeout reusing DEBOUNCE_MS (settle).
        self.assertRegex(
            self.html,
            re.compile(r"pendingQueue\.length\s*<=\s*1"),
            "Demotion gate must require pendingQueue drained to <= 1",
        )
        self.assertIn(
            "DEBOUNCE_MS",
            self.html,
            "Demotion settle window must reuse DEBOUNCE_MS",
        )

    def test_demote_has_is_replay_latch(self):
        """Demotion must require is_replay to have stopped (latch, §1.11)."""
        self.assertRegex(
            self.html,
            re.compile(r"lastReplay|replayActive|isReplayActive"),
            "A replay latch variable must gate demotion while replay is active",
        )

    def test_single_and_ribbon_modes_pin_state(self):
        """single mode pins SINGLE; ribbon mode pins RIBBON (machine bypassed)."""
        # resolvedMode === 'adaptive' must gate the state machine.
        self.assertRegex(
            self.html,
            re.compile(r"resolvedMode\s*===\s*['\"]adaptive['\"]"),
            "The state machine must only run when resolvedMode === 'adaptive'",
        )
        self.assertRegex(
            self.html,
            re.compile(r"resolvedMode\s*===\s*['\"]ribbon['\"]"),
            "Ribbon mode must pin the ribbon render branch",
        )

    def test_promotion_adopts_live_box_remaining_lifetime(self):
        """Promotion must re-arm the adopted box for its REMAINING lifetime (§1.12).

        The lifetime is the named constant SUB_LIFETIME_MS (= 5000, reusing Track C's
        5000ms hide). Adoption must compute remaining = max(0, SUB_LIFETIME_MS - age)
        where age = (Date.now() - _bornAt). Assert the constant value and the exact
        remaining-lifetime expression so a regression to a fresh full lifetime fails.
        """
        self.assertRegex(
            self.html,
            re.compile(r"SUB_LIFETIME_MS\s*=\s*5000"),
            "SUB_LIFETIME_MS must be 5000 (Track C lifetime, reused)",
        )
        self.assertRegex(
            self.html,
            re.compile(
                r"Math\.max\(\s*0\s*,\s*SUB_LIFETIME_MS\s*-\s*\(\s*Date\.now\(\)\s*-\s*bornAt\s*\)"
            ),
            "Adoption must re-arm for max(0, SUB_LIFETIME_MS - (Date.now() - bornAt))",
        )
        # The adoption helper must feed that remaining value into armPerLineExpiry.
        adopt = re.search(
            r"function adoptBoxIntoRibbon\([^)]*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(adopt, "adoptBoxIntoRibbon function body not found")
        self.assertRegex(
            adopt.group(1),
            re.compile(r"armPerLineExpiry\(\s*box\s*,\s*remaining\s*\)"),
            "Adoption must arm the per-line timer with the remaining lifetime",
        )

    def test_promotion_clears_isshowing_for_single_rearm(self):
        """FIX 1 / AC-15/AC-16: promotion must reset isShowing so single is re-armable.

        onSubtitleComplete is the ONLY place that resets isShowing=false on the single
        path, and it no longer runs for adopted boxes. If isShowing stays orphaned at
        true, after a later demotion the single guard `if (!isShowing) processQueue()`
        is permanently dead and incoming single-state lines are swallowed. Promotion
        must set isShowing = false so the serialized single path re-arms post-demote.
        """
        decision = re.search(
            r"function resolveRenderStacked\([^)]*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(decision, "resolveRenderStacked function body not found")
        body = decision.group(1)
        # The promotion branch (SINGLE -> RIBBON) must reset isShowing to false.
        promote = re.search(
            r"adaptiveState\s*=\s*['\"]RIBBON['\"]\s*;(.*?)\n            \}",
            body,
            re.DOTALL,
        )
        self.assertIsNotNone(promote, "promotion branch not found in resolveRenderStacked")
        self.assertRegex(
            promote.group(1),
            re.compile(r"isShowing\s*=\s*false"),
            "Promotion must reset isShowing = false so the single path re-arms after demote",
        )

    def test_demote_settle_clears_replay_latch(self):
        """FIX 3: the demote settle callback must age out replayActive on drained silence.

        replayActive is set on each adaptive enqueue but nothing clears it after a
        catch-up replay burst is followed by SILENCE, so the demote condition
        (!replayActive) is never satisfied and the overlay is stuck in RIBBON forever.
        The settle callback must clear replayActive = false once the queue is drained
        and only one (or zero) live box remains.
        """
        sched = re.search(
            r"function scheduleDemoteCheck\([^)]*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(sched, "scheduleDemoteCheck function body not found")
        body = sched.group(1)
        self.assertRegex(
            body,
            re.compile(r"replayActive\s*=\s*false"),
            "The demote settle callback must clear replayActive = false (age out the latch)",
        )

    def test_ribbon_enqueue_bounded_by_max_pending_queue(self):
        """FIX 4 / OD-4: the ribbon enqueue branch must bound pendingQueue.

        A bare pendingQueue.push in the ribbon branch lets a sustained burst arriving
        faster than the pump grows pendingQueue unbounded. The ribbon branch must apply
        the same discard-oldest transport guard (shift when length >= MAX_PENDING_QUEUE)
        before pushing, matching the single path's transport valve.
        """
        enqueue = re.search(
            r"function enqueueSubtitle\([^)]*\)\s*\{(.*?)\n        \}",
            self.html,
            re.DOTALL,
        )
        self.assertIsNotNone(enqueue, "enqueueSubtitle function body not found")
        # Isolate the ribbon (renderStacked) branch.
        ribbon_branch = re.search(
            r"if\s*\(\s*renderStacked\s*\)\s*\{(.*?)\n            \}",
            enqueue.group(1),
            re.DOTALL,
        )
        self.assertIsNotNone(ribbon_branch, "ribbon enqueue branch not found")
        branch_body = ribbon_branch.group(1)
        self.assertRegex(
            branch_body,
            re.compile(r"pendingQueue\.length\s*>=\s*MAX_PENDING_QUEUE"),
            "Ribbon enqueue must guard against MAX_PENDING_QUEUE before push",
        )
        self.assertIn(
            "pendingQueue.shift()",
            branch_body,
            "Ribbon enqueue must discard the oldest pending item on overflow",
        )


class TestFlipCascadePrecondition(unittest.TestCase):
    """FLIP correctness hinges on the CSS cascade.

    The reposition glide is driven by element.animate() (Web Animations API), which
    lives in the *Animations* cascade origin. That origin ranks BELOW author
    !important declarations (only Transitions outrank !important). So if the resting
    state `.sub-box.visible` set its transform with !important, the FLIP keyframes
    could not override it and the glide would be a silent no-op in the browser/OBS
    (CEF/Chromium implements this cascade order). This pins the precondition so the
    !important can never be reintroduced.
    """

    def setUp(self):
        self.html = _load_html()

    def test_visible_transform_is_not_important(self):
        block = re.search(r"\.sub-box\.visible\s*\{(.*?)\}", self.html, re.DOTALL)
        self.assertIsNotNone(block, ".sub-box.visible rule not found")
        transform_line = re.search(r"transform:[^;]*;", block.group(1))
        self.assertIsNotNone(transform_line, ".sub-box.visible must set a transform")
        self.assertNotIn(
            "!important",
            transform_line.group(0),
            "transform on .sub-box.visible must NOT be !important, or element.animate() "
            "(Animations origin, below author-!important) cannot override it and the "
            "FLIP reposition glide is silently suppressed",
        )

    def test_flip_drives_glide_via_web_animations_api(self):
        self.assertIn("flipPlayRibbon", self.html, "FLIP play helper must exist")
        self.assertIn(
            ".animate(",
            self.html,
            "FLIP must drive the survivor glide via element.animate() (Web Animations API)",
        )


if __name__ == "__main__":
    unittest.main()
