# Design: Public Launch Readiness

## Technical Approach

Implement launch readiness as a bounded remediation slice across repository metadata, public artifacts, dependency manifests, tests, and docs. Follow the existing LiveAudio pattern of small direct file edits plus test coverage, and treat launch governance as code: if a public target, dependency, or artifact hygiene rule is not reflected in tracked files, the release is not ready.

## Architecture Decisions

### Decision: Keep Open Workflow Artifacts Public

**Choice**: Retain Conductor/SDD artifacts in the repository, but sanitize them.
**Alternatives considered**: Remove all workflow artifacts from the public repo; move the whole workflow private.
**Rationale**: The user wants openness by default. The actual risk is not visibility itself, but sensitive content such as local paths, private URLs, and env details.

### Decision: Separate Launch Blockers from Follow-ups

**Choice**: Treat broken build/install paths, conflicting repo targets, hanging critical tests, and sensitive tracked data as blockers; treat the pending ASR-language manual verification as a documented follow-up.
**Alternatives considered**: Block launch on every unfinished item.
**Rationale**: The app is currently functional. Release gates should protect trust and reproducibility, not punish unrelated backlog.

### Decision: Fix Public Setup from Source of Truth Files

**Choice**: Align `requirements.txt`, `build_portable.py`, tracked links, and docs directly rather than adding external release notes as a workaround.
**Alternatives considered**: Document exceptions without changing tracked sources.
**Rationale**: Public users clone the repo and trust tracked files. If the source-of-truth files lie, the release is not reproducible.

### Decision: Enforce Verification with Strict TDD-Compatible Checks

**Choice**: Repair or replace the hanging network test and add targeted checks for public-artifact hygiene where code changes justify them.
**Alternatives considered**: Accept partial green status or rely only on manual review.
**Rationale**: A public release needs an executable baseline. Hanging validation destroys confidence in future maintenance.

## Data Flow

Launch readiness is a verification flow, not a runtime feature:

    Launch target decision
            │
            v
    Source-of-truth files updated
    (.git remote, main.py, requirements, build script, docs)
            │
            v
    Hygiene review
    (.atl + conductor/openspec public artifacts)
            │
            v
    Automated validation
    (targeted tests + compile/build sanity)
            │
            v
    Release checklist outcome
    blocker | non-blocking follow-up

## File Changes

| File | Action | Description |
|------|--------|-------------|
| `requirements.txt` | Modify | Add missing runtime dependencies required by shipped code. |
| `build_portable.py` | Modify | Keep portable build dependency installation aligned with runtime imports. |
| `main.py` | Modify | Confirm canonical public link target and preserve current branding behavior. |
| `.atl/skill-registry.md` | Modify | Remove maintainer-local filesystem paths from tracked public artifact content. |
| `tests/test_network.py` | Modify | Replace hanging acceptance logic with deterministic async verification. |
| `README.md` | Modify | Document current launch-relevant setup and follow-up status honestly. |
| `CHANGELOG.md` | Modify | Record launch-readiness fixes and non-blocking follow-up notes. |
| `conductor/tracks.md` | Modify | Mark ASR-language manual verification as non-blocking launch follow-up if needed. |

## Interfaces / Contracts

The release contract is file-based:

```text
Canonical public target: https://github.com/plynte-labs/LiveAudio
Public artifacts: allowed if sanitized
Launch blocker classes:
1. conflicting public destination
2. sensitive tracked artifact
3. undeclared runtime dependency
4. hanging/failing critical validation
Non-blocking follow-up:
- incomplete item that does not break core app functionality
```

## Testing Strategy

| Layer | What to Test | Approach |
|-------|-------------|----------|
| Unit | Dependency/build alignment | Add or update assertions around declared runtime requirements where feasible. |
| Unit | Network test determinism | Rewrite the localhost acceptance test so it terminates deterministically under pytest/unittest. |
| Integration | Public artifact hygiene | Validate that tracked artifacts no longer expose local machine paths or private identifiers. |
| Integration | Launch docs consistency | Verify canonical destination appears consistently in app links/docs after edits. |
| Build sanity | Python source validity | Run `python -m compileall main.py core utils`. |

## Migration / Rollout

No runtime migration required. Rollout is operational: keep the repo private, complete blocker fixes, rerun validations, then update the remote/public destination under `plynte-labs`.

## Open Questions

- [ ] None.
