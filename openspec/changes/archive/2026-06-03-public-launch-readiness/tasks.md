# Tasks: Public Launch Readiness

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 180-320 |
| 400-line budget risk | Low |
| Chained PRs recommended | No |
| Suggested split | Single PR |
| Delivery strategy | ask-on-risk |
| Chain strategy | pending |

Decision needed before apply: No
Chained PRs recommended: No
Chain strategy: pending
400-line budget risk: Low

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Sanitize artifacts, align dependencies, fix tests, and update docs | PR 1 | Single bounded slice with tests and release docs together |

## Phase 1: Release Contract Foundation

- [x] 1.1 RED: add/update tests that detect missing runtime dependency declarations and canonical public repo references.
- [x] 1.2 GREEN: update `requirements.txt`, `build_portable.py`, and any release-target references to match `https://github.com/plynte-labs/LiveAudio`.
- [x] 1.3 REFACTOR: keep dependency declarations and public-target strings consistent without duplicating mismatched values.

## Phase 2: Public Artifact Hygiene

- [x] 2.1 RED: add a deterministic check for tracked local/private path leakage in `.atl/skill-registry.md` or equivalent tracked public artifacts.
- [x] 2.2 GREEN: sanitize `.atl/skill-registry.md` and related release-facing workflow artifacts so they keep value without exposing personal/private data.
- [x] 2.3 GREEN: update `conductor/tracks.md` and related notes so ASR-language verification stays visible as a non-blocking follow-up.

## Phase 3: Verification Baseline Repair

- [x] 3.1 RED: rewrite `tests/test_network.py` localhost-acceptance coverage so the expected behavior is asserted without an infinite wait.
- [x] 3.2 GREEN: implement the minimal test-side or code-side change needed so the network verification completes deterministically.
- [x] 3.3 REFACTOR: rerun focused network tests and remove any brittle async assumptions that could hang future release checks.

## Phase 4: Documentation and Launch Notes

- [x] 4.1 GREEN: update `README.md` with launch-relevant setup truth, including current public target and any public-artifact hygiene expectations.
- [x] 4.2 GREEN: update `CHANGELOG.md` with launch-readiness fixes and clearly label non-blocking follow-up items.
- [x] 4.3 GREEN: add or adjust any launch-facing docs needed so public maintainers understand current release status without reading code.

## Phase 5: Final Validation

- [x] 5.1 GREEN: run `python -m pytest -q` and confirm the launch-critical automated baseline completes.
- [x] 5.2 GREEN: run `python -m compileall main.py core utils` to verify source validity after edits.
- [x] 5.3 REFACTOR: review the final diff against the launch spec and confirm remaining items are documented as blockers or non-blocking follow-ups.
