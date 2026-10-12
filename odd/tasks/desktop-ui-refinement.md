# Desktop UI refinement

## Objective
Make the existing Rust/Tauri desktop interface easier to configure and visually calmer while preserving the dark/emerald identity and two-column workspace.

## Problem and intent
The current interface gives technical settings and diagnostic indicators too much emphasis. First-time users need a clear source/language/profile path, readable guidance, and predictable settings behavior.

## Authorized scope and constraints
- User approved the in-chat redesign and requested Luna subagents.
- Route: delegated direct, not an SDD pipeline. Four Luna workers consulted before implementation; one writer at a time.
- Modify desktop HTML/CSS/JavaScript, frontend tests, relevant documentation, and a minimal no-argument native command for the fixed author-profile URL. No other backend/config/protocol changes are authorized for the credit.
- Preserve configuration keys, OBS/WebSocket protocol, service lifecycle, privacy defaults, English/Spanish localization, and saved-versus-active/restart consent.
- Keep model, CPU/fallback, VAD, and backlog controls accessible in Advanced.
- Group session folder, output formats, and session continuity under Files.
- No native app/microphone/model execution, PR, release publication, force push, or history rewrite. Installer build and scoped develop integration are now authorized as below.
- Do not add preview-only font-size/position controls: those require a separately designed end-to-end configuration/overlay change.
- Strict TDD: enabled by AGENTS.md. Runner: `node desktop/test_frontend.js`; observed RED before new behavior, GREEN, then refactor.

## Branch and delivery
- Branch: `feature/desktop-ui-refinement`, based on `feat/rust-tauri-migration` at `a89201046dab258d2c8aa083ff426f9ea08fd39a`.
- RDD: off, decided by global mode; no native review transaction or consent ceremony.
- Delivery strategy updated by explicit user request: one squash commit on develop, including all 37 prior commits plus the current UI changes. No chained PR or PR creation requested.
- Authorized remote: https://github.com/plynte-labs/LiveAudio.git, branch develop, using the configured GitHub CLI session only for consultation/fetch and normal push. No force push, release publication, or unrelated destinations.
- Verified origin/develop: c155e9e791d4eec38644792fa7a8deb00ca4c8e7; ancestor of the current base with 37 local-only commits and zero remote-only commits. User explicitly confirmed inclusion of all historical work (240 files before UI changes).
- Local develop at 82066005d0cc62a6292646728697e89240622e3b has three unpublished commits, all present in the feature history. Preserve that local branch; use a separate delivery branch from origin/develop for the single squash commit, then push that commit to remote develop normally. Do not reset or force-update the existing local branch.
- Initial forecast: 150–300 authored changed lines from mapping; revise after writer inspection. The ~400-line task heuristic must not cause code golf or omitted tests.
- Commits require explicit auditor approval after fresh checks. User resolved the delivery strategy as a single complete squash on remote develop; preserve the original feature/local develop histories and do not rewrite remote history.

## Tasks
- [x] UI-1: Reorganize existing settings into Setup, Subtitles, Advanced, and Files; keep one primary service status, preserve drafts and settings semantics, and make navigation keyboard-accessible. Route: delegated; trigger: multiple non-trivial HTML/JS/i18n/test files. Checks: strict TDD regression coverage, including exhaustive app.js literal-ID-to-HTML mapping and Ribbon setting persistence wiring; existing frontend suite, JS syntax, duplicate-ID/control mapping review.
- [ ] UI-2: Apply restrained surfaces, consistent spacing/type, readable hints, visible focus, reduced motion, responsive layout, and concise OBS help; update user-facing guidance. Route: delegated; trigger: multi-file design work. Checks: frontend suite, CSS/HTML structural checks including main's direct pane children, localized tablist accessible name, static contrast, independent review; rendered UI verification may remain pending if browser tools fail.
- [x] UI-3: Add `by Franguh` beside the version, linking to https://github.com/franguh with safe, accessible external navigation. Route: delegated; trigger: markup/behavior/tests. Checks: observed RED/GREEN, frontend/syntax checks and link handling review.
- [x] UI-4: Generate and inspect the Windows NSIS installer using the existing Tauri packaging pipeline. Route: delegated; trigger: build execution and packaging analysis. Checks: exact build result, artifact path/size/hash, package resources and clean-device limitations; no native application or ASR launch.
- [ ] UI-5: Preserve feature work in a checkpoint, integrate the complete confirmed range as one conventional squash commit on develop, verify the integrated source, and push normally to the authorized origin using gh credentials. Route: auditor Git management plus delegated functional verification. Checks: fresh required suites, local/remote commit identity, no force push or unrelated changes.

## Acceptance criteria
- Setup foregrounds audio source, spoken language, and the existing recommended profile.
- Every existing setting remains loadable/saveable exactly once; switching tabs or profiles does not silently discard unrelated draft edits.
- Selected tab, visible panel, ARIA state, and keyboard navigation agree.
- Header summaries are concise and full source details remain available without misleading consumer identities.
- Files controls share one coherent location; output defaults and saved choices remain unchanged, with JSONL/WebVTT controls explicitly configurable.
- Visual changes preserve two columns at supported desktop widths and avoid horizontal overflow; Spanish/English labels remain coherent.
- No fabricated progress percentages, HTTP overlay substitution, or unsupported persistence promises.

## Progress and evidence
- Four read-only Luna workers mapped implementation, safety, QA, and visual design.
- Working tree was clean at branch point; no user changes to preserve. Implementation is on `feature/desktop-ui-refinement`.
- Baseline: `node desktop/test_frontend.js` — 106 passed, 0 failed.
- Strict-TDD RED observed before adding `bindTabNavigation`: importing the missing named export failed with `SyntaxError: The requested module './src/runtime-settings.js' does not provide an export named 'bindTabNavigation'`.
- UI-1 implementation: reorganized the existing controls into Setup, Subtitles, Advanced, and Files; added keyboard-operable tab/tabpanel state; profiles now read the current form draft before preset fields apply. Regression suite verifies group membership, unique IDs, ARIA/keyboard sync, draft preservation, and restart/save semantics.
- UI-2 implementation: restrained the two-column dark/emerald UI, consolidated header indicators under Details, improved text/focus contrast, added reduced-motion handling, compacted OBS guidance, and updated desktop guidance/changelog/README.
- Verification: `node --check desktop/src/app.js`, `node --check desktop/src/runtime-settings.js`, `node --check desktop/src/i18n.js`, `node desktop/test_frontend.js` — 122 passed, 0 failed; `git diff --check` passed (Git reports LF-to-CRLF normalization warnings only).
- Current tracked diff after correction: 601 insertions + 316 deletions = 917 changed lines across 9 tracked files (task document is untracked and excluded from Git's diff stat); above the ~400-line delivery budget, so do not commit until the auditor resolves the `ask-on-risk` delivery strategy.
- Independent verification and four Luna sign-offs completed after the scoped correction; all are PASS-with-notes. Rendered visual verification remains pending; the app was not launched.
- Work-unit commits: none.
- Engram mirror: maintained by the parent auditor.

### Consolidated review correction (no commit)
- Initial independent QA reported FAIL: `index.html` had removed `slider-ribbon-lines` and `val-ribbon-lines` although `app.js` still used them for population, config collection, and input binding.
- Added TDD regression checks before production edits for all literal `app.js` `getElementById` targets mapping exactly once to HTML, Ribbon settings placement/config binding, localized tablist accessible name, OBS-details focus styling, and truthful session/output guidance.
- Correction RED: `node desktop/test_frontend.js` — 123 passed, 8 failed; failures included missing ribbon IDs/control, missing localized aria label handling and translations, missing OBS-details focus rule, and stale/absent documentation claims.
- Correction GREEN: after restoring the existing 1–8 Ribbon line control with value display in Subtitles, localizing `aria-label` through existing i18n, adding the OBS summary focus selector, and correcting session-path/output-default guidance, `node desktop/test_frontend.js` — 131 passed, 0 failed.
- Final syntax/whitespace checks: `node --check desktop/src/app.js`, `node --check desktop/src/runtime-settings.js`, `node --check desktop/src/i18n.js`, and `git diff --check` — passed; Git printed LF-to-CRLF warnings only.
- The first post-edit run was 130 passed / 1 failed because the new documentation assertion expected a different phrase than the corrected paragraph; the test phrase was aligned and the final GREEN run passed.
- This review also corrected a prior acceptance assumption: Rust config defaults determine JSONL/WebVTT sinks; the desktop continues to preserve those defaults and exposes saved user choices. No backend defaults or config behavior changed.
- Independent QA reran the final frontend suite (131 passed, 0 failed), all three JS syntax checks, and whitespace validation: passed. Auditor spot-check `node --check desktop/src/app.js`: passed.
- Architecture, QA, Design, and Performance/Research each returned PASS-with-notes after correction. Remaining notes: rendered responsive behavior and expanded OBS help clipping in short windows require manual verification; no native app, microphone, models, or OBS session were executed. Rust checks were not run because no Rust files changed.
- Native read-only risk assessment: medium; RDD remains globally off. Independent verification was required for the Luna/small-model implementation and performed. Initial assessment needed an explicit untracked task-file inventory; assessment then succeeded. No native review lifecycle was started.
- Preparing a temporary source-diff review package was rejected by auto-review; no copy was made. Reviewers inspected the authorized worktree directly instead. No review remains blocked by this rejected transport.
- UI-1 and UI-2 source work and automated checks are implemented, but task checkboxes stay open pending rendered validation and the delivery/commit decision. No commits or remote operations occurred.

### Responsive layout regression from supplied screenshot (UI-2 reopened; corrected, no commit)
- A later supplied screenshot at roughly 1918px viewport showed the settings/preview content occupying only the left ~980px while the right grid track was blank; this supersedes the prior source-only responsive confidence and reopens UI-2 pending new independent validation.
- Root cause: `main` had the existing two-column CSS grid, but `.right-pane` was nested inside `.left-pane` in `index.html`. The tab-card and tab-content closed around lines 321–322; `.left-pane` itself was not closed before `.right-pane` began around line 325, leaving only one grid item.
- Added a test-only HTML stack parser (skipping comments and not pushing void tags) and regression assertion that `.left-pane` and `.right-pane` are direct children of `<main>`.
- Responsive-layout RED: `node desktop/test_frontend.js` — 131 passed, 1 failed; the sole failure was the new direct-sibling hierarchy assertion.
- Corrected the source with one missing `</div>` between the tab card and right pane; CSS breakpoints/grid rules are unchanged. GREEN: `node desktop/test_frontend.js` — 132 passed, 0 failed. The modal/toast blocks remain after `</main>` in the app root.
- `node --check desktop/src/app.js`, `node --check desktop/src/runtime-settings.js`, `node --check desktop/src/i18n.js`, and `git diff --check` — passed; Git emitted LF-to-CRLF warnings only.
- Browser/rendered recheck at wide and narrow viewports remains pending; no application, microphone, model, or OBS runtime was launched. No backend/config/network changes or commits were made.
- Updated tracked diff count: 601 insertions + 316 deletions = 917 changed lines across 9 files (task document remains untracked and excluded from Git's stat).

### UI-3 author credit (awaiting parent approval; no commit)
- Added a semantic `by Franguh` anchor immediately after the version badge in both EN and ES, with a safe HTTPS profile URL, `noopener noreferrer`, a localized accessible name, visible keyboard focus, and explicit no-drag behavior.
- In Tauri, the click is prevented from navigating the WebView and invokes no-argument `open_author_profile`; Rust keeps the URL fixed and dispatches to the platform's default browser handler (Windows `rundll32.exe`, macOS `open`, other platforms `xdg-open`). No plugin/dependency or arbitrary frontend URL/path input was added. Browser-preview mode retains the ordinary anchor behavior.
- Failures surface a localized English/Spanish toast. A Rust unit test verifies the fixed destination and platform command arguments without spawning a browser.
- Strict-TDD RED: frontend regression suite failed the new assertions (133 passed, 11 failed); focused Rust compile/test failed because `author_profile_open_command` was absent. GREEN: `node desktop/test_frontend.js` — 144 passed, 0 failed; `node --check desktop/src/app.js`, `node --check desktop/src/runtime-settings.js`, `node --check desktop/src/i18n.js` — passed; `cargo test -p liveaudio-desktop --offline --locked` — 26 passed, 0 failed; `git diff --check` — passed with LF-to-CRLF normalization warnings only.
- `rustfmt --edition 2021` was run only on the three touched Rust files before functional verification. Cargo emitted existing incremental-cache hard-link fallback warnings; tests passed.
- No app/browser launch was performed, so actual system-browser handoff and rendered header fit remain manual QA. UI-3 remains unchecked until parent review; no commit or remote operation occurred.
- Final copy correction: preserve the user's exact literal `by Franguh` label in both languages. The new EN/ES parity regression was RED at 143 passed / 1 failed, then GREEN at 144 passed / 0 failed; all three JS syntax checks passed. No Rust source changed for this correction.

## Next step
UI-3 received Architecture PASS and independent QA PASS: Rust workspace 132 passed, frontend 144 passed, focused Python 203 passed plus 24 subtests; three JS syntax checks and whitespace check passed. Auditor syntax spot-check passed. The literal `by Franguh` is retained in both locales. Actual browser handoff and rendered header fit remain manual checks.

Auditor approves a local feature checkpoint containing only the reviewed feature files and task record. Build source is frozen after that checkpoint; no source-mutating normalizers may run during verification/build. Build tool preparation resolved ephemeral Tauri CLI 2.12.1 through npm; NSIS is already cached. Windows NSIS release compilation is expected to take minutes. The installer does not bundle managed Python/uv/ASR dependencies; do not claim offline or clean-device transcription readiness. Build results, artifact hash, and subsequent single-commit develop delivery remain pending.


### Delivery resume and packaging correction
- Feature checkpoint: 1c7ce886b0b26728a2664c2e7bf259ee13662dfd. Fresh independent checks: Rust 132 passed; frontend 144 passed; Python 203 passed plus 24 subtests; syntax and whitespace passed.
- NSIS build failed after release compilation with invalid category; no installer produced. CLI-only manifest normalization was restored, retaining frozen source.
- Generated resource path is _up_/_up_/liveaudio, absent from installed worker lookup; package glob also includes unnecessary bytecode caches. UI-4 includes minimal packaging configuration correction and regression checks before rebuild. Route: one delegated writer; strict TDD enabled, node desktop/test_frontend.js and relevant packaging checks.
- Engram mirror updates are pending because runtime session identity is unavailable; no agent-attributed memory writes are permitted until host registration.

- Packaging repair: supported Music category; explicit direct-file source-to-target maps place Python modules at liveaudio/ and exclude nested bytecode caches. RED: 145 passed/3 failed; GREEN and independent spot-check: 148 passed/0 failed. Independent safety review confirmed primary Tauri mapping semantics and compatibility of existing packaging validation. NSIS artifact proof still pending.

- Rebuild confirmed generated NSIS resource directives use liveaudio/service/asr_worker.py and exclude bytecode caches, then failed because locale codes en-US/es-ES were incorrectly used as NSIS language names. Minimal repair uses cached English and SpanishInternational language names; RED149/1, GREEN150/0. Installer rebuild remains pending.


### Final artifact and delivery candidate
- Source checkpoint22e12312df8e6f3bbfab9ff29a973f1fa9264e97. Installer build exit0: target/release/bundle/nsis/LiveAudio_1.5.0_x64-setup.exe, 3872469 bytes; SHA256 1A2B37616AA21CFE9065ACC7BA2146B4890359459E53DAEC2DD62937465A5D37. Authenticode NotSigned.
- Generated NSIS script has34 runtime resource directives, correct liveaudio/service/asr_worker.py destination, zero bytecode-cache references. Archive extraction unavailable (7-Zip absent); no app/install/ASR launch.
- Final independent verification: Rust132, frontend150, Python203 plus24 subtests, JS syntax and whitespace all exit0. Parent frontend spotcheck150/0. CLI-only manifest normalization restored, source tree clean before integration.
- UI-1/UI-3/UI-4 automated implementation and artifact work closed. UI-2 remains open solely for rendered wide/narrow and header/browser manual verification; UI-5 remote push/identity proof pending. Managed Python/uv/ASR dependencies are not bundled; clean-device transcription remains untested and requires provisioning.
- Delivery branch created from origin/develop c155e9e791d4eec38644792fa7a8deb00ca4c8e7 with complete feature history squashed. Existing local develop and feature histories preserved. Engram mirror remains pending host session registration.

- Full squash cached whitespace check reports inherited historical trailing/EOF whitespace in conductor/tracks.md, audio/vad Cargo manifests, ADR-016 draft, packaging_wu8 guide, and asr_worker_protocol.py. These bytes are unchanged versus the preserved feature source and are not new UI/packaging defects; no historical cleanup or post-build normalization performed. Functional verification on the integrated tree is pending before push.

- Integrated source independently verified: Rust132/0, frontend150/0, Python203 plus24 subtests, all syntax checks exit0. Source comparison excluding this ledger returns zero paths. Inherited staged whitespace check remains failed as documented; ordinary worktree whitespace check passes. Auditor approves the single complete squash commit and normal develop push authorized by the user.
