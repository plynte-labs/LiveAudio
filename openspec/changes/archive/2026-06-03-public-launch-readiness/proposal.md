# Proposal: Public Launch Readiness

## Intent

Prepare LiveAudio for a safe public release under the target organization/repository with consistent governance, reproducible setup, and verifiable launch quality.

## Confirmed Decisions

- Canonical public destination: `https://github.com/plynte-labs/LiveAudio`
- Conductor/SDD artifacts MAY remain public if they do not contain sensitive URLs, private environment details, or personal/critical information.
- The pending ASR language verification track is a known follow-up, but it is NOT a launch blocker because the app is currently functional.

## User Story

As a maintainer preparing LiveAudio for public release, I want a formal launch-readiness change that captures repository, licensing, privacy, QA, and build blockers so that we can open the project without exposing private data, broken setup paths, or unverifiable behavior.

## Scope

### In Scope
- Define the launch blockers that must be closed before the repository becomes public.
- Formalize repository-governance requirements for canonical org/repo, links, license alignment, and tracked artifacts.
- Formalize release-quality requirements for tests, docs, manual QA, and reproducible installation/build.
- Audit public Conductor/SDD artifacts for sensitive-content hygiene instead of removing them by default.

### Out of Scope
- Executing the final org transfer or making the repository public.
- Shipping unrelated product features beyond launch-readiness blockers.

## Capabilities

### New Capabilities
- `public-launch-readiness`: Release gate for repository governance, privacy hygiene, dependency/build integrity, documentation completeness, and verification status before public launch.

### Modified Capabilities
- None.

## Approach

Create a launch-readiness spec first, then design a remediation sequence that follows strict TDD for code-affecting fixes and explicit manual QA gates for public-release checks. Keep governance decisions and code/runtime fixes traceable but separable.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `.git` remote config | Modified | Canonical repository target must be defined and aligned with public links. |
| `main.py` | Modified | Public GitHub links and Pillow runtime dependency must align with release target. |
| `requirements.txt`, `build_portable.py` | Modified | Missing Pillow dependency blocks clean install/build. |
| `.atl/skill-registry.md` | Modified | Tracked local filesystem paths must not ship publicly. |
| `conductor/tracks.md`, `conductor/tracks/asr-language-separation_20260601/` | Modified | Pending manual verification should be documented as non-blocking follow-up for launch. |
| `README.md`, `CHANGELOG.md`, `docs/` | Modified | Launch-critical behavior and release notes are incomplete. |
| `tests/test_network.py` | Modified | Hanging test prevents a trustworthy green suite. |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Public repo exposes private/local machine paths | High | Remove or untrack private artifacts before launch. |
| First-time install/build fails publicly | High | Add missing dependencies and verify from clean flow. |
| Launch claims diverge from actual repo target or product behavior | High | Align remote, UI links, docs, changelog, and release checklist. |

## Open Questions

- None for proposal scope. Remaining uncertainties should move into the spec as explicit requirements or follow-up notes.

## Rollback Plan

Keep the repository private and postpone transfer/publication until all launch blockers are resolved or explicitly accepted.

## Dependencies

- Repository/org access to publish under `plynte-labs`.
- Manual QA availability for any launch checks that remain mandatory after the spec phase.

## Success Criteria

- [ ] Launch blockers are captured in an auditable SDD artifact.
- [ ] The canonical public repo target and artifact hygiene policy are decided.
- [ ] Non-blocking follow-ups are explicitly documented so they are not confused with release gates.
- [ ] The change is ready to proceed into spec/design/tasks with strict TDD for code-impacting fixes.
