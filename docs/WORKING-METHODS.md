# Confirmed Working Methods

This ledger is the source of truth for behavior the user has confirmed in real use. It separates a tested candidate from a working baseline and keeps every promoted baseline recoverable.

## Promotion procedure

1. Before editing a listed flow, identify its confirmed version and rollback tag. An explicit user request naming that flow authorizes one candidate change. Ask before changing it as part of unrelated work.
2. Keep the confirmed tag and installer intact. Record the proposed implementation as a candidate; automated tests do not promote it.
3. Validate the candidate with its focused regression tests and the full applicable suite. Give the user a test build only when requested.
4. Promote only after the user explicitly says the candidate works or confirms it as the new baseline.
5. On promotion, replace the candidate row with a numbered confirmed version, record the user's acceptance and test evidence, then run:

   `python3 scripts/capture-working-baseline.py <method-id> v<number> "<short description>"`

   The command creates an annotated `working/<method-id>/v<number>` tag containing the exact tracked and untracked repository state without changing the current index or working tree.
6. For rollback, inspect the confirmed tag against the current tree first. Restore only the affected files after an explicit rollback request; preserve later unrelated work.

## Status meanings

- **Confirmed** — the user has exercised the behavior and explicitly accepted it. It has a source tag and, when applicable, an installer path plus checksum.
- **Candidate** — implemented and possibly test-verified, but not accepted in real use. It cannot replace the confirmed version.
- **Historical evidence** — the user confirmed an older deployment, but no exact source snapshot can be proven. Preserve the evidence without inventing a tag.
- **Rejected** — the user reported that the candidate does not work. Keep its diagnosis if useful, but never use it as a rollback point.

## Method ledger

### browser-status-presentation

Scope: recording a dental status, refreshing the same patient's Status view in Chrome, recovering from a stale Chrome window, and reporting whether the saved result became visible.

Historical evidence:

- On 2026-09-17, the user reported that the deployment before the overlay-related changes worked perfectly.
- The repository does not identify that deployed source revision with enough certainty, so it is not represented as a recoverable source tag.
- Preserve the existing test installers under `release/test-builds/`; neither timestamped build is labelled as the historical baseline until the user identifies it.

Preserved but unclassified installers:

- `release/test-builds/20260917-232357/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — SHA-256 `a8ca64898a4610d5f157959ec83bd2c24cc7e9ff995cb1352c81c2b408903904`
- `release/test-builds/20260917-234007/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — SHA-256 `8af98b8c1f27938f18bfd20f62cddb62a1e5f6d95dcc06b3010954905c379473`

Preserved candidate `C1`:

- Same patient: refresh the current AIDOO tab.
- New patient: open a new tab.
- Stale managed window: rediscover a live AIDOO window before opening a dedicated Chrome window.
- A verified clinical write waits for Chrome to confirm the exact patient and view before it reports visible completion.
- Browser failure remains separate from write verification, so a saved clinical change is not repeated.
- Automated evidence: the focused stale-window and visible-completion regression tests pass; the full Rust suite has 145 passing tests; frontend checks and the production build pass.
- Test installer: `release/test-builds/20260918-001633/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — notarized, stapled and independently audited; SHA-256 `f4b08242ca7db26f0e7536392102e75985e982807481f39c1bdff46b1cca58a1`.
- State: awaiting user test and explicit confirmation. No confirmed tag exists for `C1`.

Rejected candidate `C2` (inherits `C1`):

- Status dictation is processed per utterance. A normal one-tooth utterance is written and visibly refreshed immediately; multiple changes in the same utterance remain one grouped API request and one refresh.
- A verified status that is visible in Chrome answers only „Записах.“. An invalid, unclear or rejected status answers only „Повтори.“.
- A verified write whose Chrome refresh fails keeps the distinct warning „Записано е, но картонът не се опресни на екрана.“ so the user is not encouraged to duplicate the write.
- Official-note dictation ends immediately on „Готово“, „Край на забележката“ or another reviewed ending phrase; the ending phrase is removed from the saved note.
- Without an ending phrase, 10 seconds of silence asks exactly „Да завършвам ли забележката?“; a negative answer resumes dictation.
- While a note is held, its transcript is presented in the active AIDOO „Забележка“ field through macOS Accessibility. The lookup accepts the real AIDOO structure where the focused element is the surrounding note container and the editable field is its named descendant.
- If the AIDOO note field is not available, the same transcript remains visible in the movable overlay. The final note is always written through the verified AIDOO API path, after which the Treatment view must visibly refresh before completion is reported.
- Automated evidence: 149 Rust tests, 46 frontend tests, strict TypeScript checks, Clippy with warnings denied, release-configuration checks and the production frontend build pass.
- Test installer: `release/test-builds/20260918-004812/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — signed, notarized, stapled and independently mounted/audited; SHA-256 `3b4dfeed14aac766b50acc720f618758ab340618914f8dbcbfd6d6ff6b0c9072`.
- Rejected on 2026-09-18 after the installed app opened repeated Chrome windows instead of refreshing the existing AIDOO view. Chrome did not expose `AXWindowNumber`; the presentation code treated the live AIDOO window as closed and repeatedly entered the dedicated `--new-window` fallback.
- Preserve the notarized installer as failure evidence only. It is not a rollback point and must not be promoted.

Rejected candidate `C3` (inherits the clinical and dictation behavior from `C2`):

- Same patient: refresh the current AIDOO tab. Different patient: open a new tab inside the existing AIDOO Chrome window.
- Reuse a Chrome AIDOO window by exact Accessibility window number when available, or by its AIDOO title when Chrome omits `AXWindowNumber`.
- Discover minimized and off-screen AIDOO windows as reusable surfaces instead of treating them as closed.
- When an AIDOO surface exists but cannot be refreshed, return a visible presentation error and never create another Chrome window automatically.
- Fall back to a regular Chrome window or create the initial dedicated AIDOO window only when no AIDOO surface exists.
- Automated evidence: the repeated-refresh regression creates zero new windows across 20 refreshes; exact patient/view, stale-window recovery, same/new-patient routing, and missing-`AXWindowNumber` regressions pass; the full suite has 152 Rust and 46 frontend tests; strict TypeScript, source-layout, localization, release-configuration, macOS dependency, wake-word, website, Clippy-with-warnings-denied, production build, diff and debug-marker checks pass.
- Test installer: `release/test-builds/20260918-010401/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — signed, notarized, stapled and independently mounted/audited; SHA-256 `db25a5563b45cb6e2c473b1dc160ad5ffecfbeeeeb2c782d26b149abffc60f46`.
- Rejected on 2026-09-18 after the installed-app test showed that a fragmented „Край на забележката“ could end the whole AI session and status dictation was still delayed instead of being written tooth by tooth. Preserve the notarized installer as failure evidence only.

Superseded candidate `C4` (inherits the same-window Chrome fix from `C3`):

- Bare or fragmented „Край“ never ends the AI session. „Край на забележката“ finishes only the official note and leaves the session active; explicit phrases such as „Приключваме“, „Затвори“ and „Довиждане“ still end the session.
- Each utterance for one tooth calls `apply_aidoo_statuses` immediately. One or several statuses for that tooth remain one grouped write, one independent read-back and one visible browser refresh.
- After verified visible completion, the spoken result repeats the tooth and every saved status, for example „Записах: Кариес на зъб едно шест и Обтурация на зъб едно шест.“. Rejection still says exactly „Повтори.“ and a failed refresh keeps the no-duplicate warning.
- Missing teeth are not queried at the first tooth or after every tooth. Only when dictation moves to a tooth in the next quadrant may the assistant, after saving that new tooth, ask about unaddressed teeth in the quadrant just left.
- Automated evidence: the fragmented-note-ending, immediate grouped status, repeated-status acknowledgement and quadrant-transition regressions pass; the complete suite has 152 Rust and 47 frontend tests; strict TypeScript, source-layout, localization, release-configuration, macOS dependency, wake-word, website, Clippy-with-warnings-denied, production build, diff and debug-marker checks pass.
- State: superseded before an installer was produced. It was never promoted or rejected in an installed-app test; all earlier installers remain intact.

Rejected candidate `C5` (inherits the same-window, note-ending and immediate-status behavior from `C4`):

- GPT-Live remains the voice conversation layer. Short structured AIDOO decisions are delegated to GPT-6 Luna through Responses function calling.
- Stable instructions are split into compact clinical and patient-selection blocks so the repeated prefix remains cacheable and unrelated instructions are not duplicated.
- For each dictated tooth, all status additions and replacements are sent in one `apply_aidoo_statuses` operation, followed by one independent read-back and one visible browser refresh. A correction such as „замени X с Y“ supplies `Y` as the new status and `X` as `replaceStatus`; several replacements for the same tooth remain in that single grouped operation.
- The assistant does not insert a conversational acknowledgement between the utterance and the write, does not ask for confirmation, and preserves the exact protected quadrant-transition prompts.
- Wake activation keeps the main WKWebView transparent and non-interactive in the `preparing` phase until browser `getUserMedia` has acquired the microphone. It hides the WebView only after the session advances to `connecting`, so the clinician still sees only the movable overlay.
- GPT-6 Luna backend usage is calculated at the reviewed Standard rates: $0.10/M input, $0.01/M cached input, $0.125/M cache writes and $0.50/M output, with the documented long-context multipliers above 272K input tokens.
- Automated evidence: the wake activation regression is red with the old `connecting` route and green with `preparing`; 15 wake-word tests, 3 microphone handoff tests, the focused model/cost and status-instruction tests, the complete 153-test Rust suite and all 47 frontend tests pass. Strict TypeScript, source-layout, localization, release-configuration, macOS dependency, wake-word, website, production build, Rust formatting, Clippy-with-warnings-denied, diff and debug-marker checks pass.
- Test installer: `release/test-builds/20261002-114413/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — signed, notarized, stapled and independently mounted/audited; SHA-256 `4da3ab2651d311a5f1920681255a94c636c991acb00ee70f0b27df61d6c4372d`.
- Rejected on 2026-10-02 after the user reported that commands for the same patient kept opening new Chrome tabs. On this Mac, CoreGraphics did not expose Chrome window titles; a managed-navigation failure discarded patient context and the generic Chrome recovery branch chose a new tab. Preserve this installer as failure evidence; it was never promoted.

Current candidate `C6` (inherits the voice, note-ending and clinical behavior from `C5`):

- Keep an exact retained Accessibility window and selected-tab reference per patient. Status, treatment, reads and writes select that patient's registered tab before changing its address; returning from patient B to patient A reuses A's registered tab.
- A new patient may receive one new tab. Record its binding immediately after opening it, before navigation confirmation, so a delayed or failed confirmation cannot repeat tab creation.
- Keep patient bindings on navigation timeouts. Evict only provably closed Accessibility elements or terminated Chrome processes, so stale references can recover without turning every command into another tab.
- During initial discovery, compare the live address with the requested patient before deciding to create a tab. Identical patients, changes between Status and Treatment, repeated schedule commands, and UUID letter-case differences reuse the current tab.
- Missing CoreGraphics titles or `AXWindowNumber` do not invalidate a retained tab/window. If existing Chrome surfaces cannot be controlled, report the error instead of creating more windows automatically.
- Diagnosis evidence: before the fix, 20 same-patient recovery commands with missing window titles chose 20 new tabs; repeated schedule commands also chose a new tab. Both regressions pass after the fix.
- Automated evidence: the complete 161-test Rust suite, all 47 frontend tests, strict TypeScript, source-layout, localization, release-configuration, macOS dependency, wake-word, website, production build, Rust formatting, Clippy-with-warnings-denied and diff checks pass. The final focused browser suite has 18 passing tests; the opt-in native Chrome capture probe could not exercise the live surface because the test process lacks macOS Accessibility permission.
- Test installer (includes `C6` and `W1`): `release/test-builds/20261002-135213/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — signed, notarized, stapled and independently mounted/audited; SHA-256 `817e5bc95bc0737de67658119c9a4e3271a274257860433c44a2d65746c243c2`. Adjacent `build-info.json` records the packaged executable and protected-source fingerprints.
- State: automated verification complete; awaiting an installed-app browser test and explicit confirmation. The `C5` installer and all earlier installers remain intact. In particular, verify repeated same-patient writes, patient A → B → A, and navigation-timeout recovery in the installed application before promotion.

Protected source:

- `src-tauri/src/aidoo/browser.rs`
- `src-tauri/src/aidoo/browser_routing.rs`
- `src-tauri/src/aidoo/browser_surface.rs`
- `src-tauri/src/aidoo/presentation.rs`
- `src-tauri/src/aidoo/commands.rs`
- `src-tauri/src/aidoo/protocol.rs`
- `src-tauri/src/aidoo/types.rs`

Required regression evidence on every change:

- A stale Chrome window recovers through a live AIDOO window.
- The same patient refreshes the current tab; a different patient opens a new tab.
- A Chrome AIDOO window without `AXWindowNumber` is reused by title.
- Twenty same-patient refreshes are incapable of creating twenty new windows.
- Twenty same-patient commands with missing Chrome window titles are incapable of creating twenty new tabs.
- Live-address discovery reuses the same patient across Status/Treatment transitions and UUID letter-case differences; repeated schedule commands reuse the current tab.
- A failed navigation confirmation keeps the registered patient tab, and a provably closed tab can be evicted for recovery.
- If any AIDOO surface exists but refresh fails, the flow reports an error instead of opening a new window.
- A verified backend write is not reported as visibly complete when Chrome does not refresh.
- Each successful one-tooth status repeats the tooth and every verified status; rejection reports exactly „Повтори.“.
- Follow the current `E1` session-end candidate below: standalone „Край“ ends the session, while „Край на забележката“ completes only the note. The earlier bare-„Край“ rule remains historical evidence, superseded by the user's 2026-10-03 request.
- Status writes are immediate and grouped per tooth; a missing-teeth question is allowed only after moving to the next quadrant.
- The note ending phrase is excluded, the fallback waits 10 seconds, and its exact question remains „Да завършвам ли забележката?“.
- The AIDOO note preview targets only the named note text field; when it is unavailable, the overlay transcript remains available.
- The complete Rust suite and frontend check pass before a test installer is considered ready.

### assistant-wake-presentation

Scope: quiet application launch and macOS reopen, showing only the assistant overlay after „Хей, Айдуу“, and keeping microphone preparation reliable while the main WebView is invisible.

Current candidate `W1` (accompanies browser candidate `C6`; preserves the microphone handoff from `C5`):

- On 2026-10-02 the user explicitly clarified that the assistant/overlay should appear after „Хей, Айдуу“ and the application should remain in the menu bar at startup.
- The packaged main and overlay windows start hidden and without focus, including Launch at Login. macOS Reopen cannot automatically show the main window. Explicit menu-bar Open and Settings actions remain available for setup, permissions and diagnostics.
- Wake activation still keeps the main WebView transparent, non-interactive and alive during `preparing`, then hides it after microphone acquisition. Do not replace this with an early native hide; it regresses automatic microphone startup.
- Register the frontend wake listener before draining the native pending latch. A wake event emitted during asynchronous registration is recovered after the listener becomes ready; duplicate event/ready notifications consume the same atomic latch and start only once. Disposed effects cannot drain or start a session after an asynchronous read finishes.
- Diagnosis evidence: the packaged-configuration test initially failed with `main opens on application launch`; the reopen-origin policy test also failed with the previous unconditional opening behavior. Both pass with quiet launch and the explicit-menu origin guard.
- Frontend diagnosis evidence: before the ready-drain fix, two of four deterministic race tests failed (the lost wake started zero sessions instead of one, and listener readiness did not drain the latch). All five final pending-wake tests pass, including the no-pending-request case.
- Automated evidence: 163 Rust tests, 52 frontend tests, strict TypeScript, source-layout, localization, release-configuration, macOS dependency, wake-word, website, production build, Rust formatting, Clippy-with-warnings-denied and diff checks pass. The native pending-wake and microphone-preparation regressions remain green.
- Test installer (same exact artifact as `C6`): `release/test-builds/20261002-135213/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — signed, notarized, stapled and independently mounted/audited; SHA-256 `817e5bc95bc0737de67658119c9a4e3271a274257860433c44a2d65746c243c2`. Exactly one new DMG was created for the user's request.
- State: automated verification complete; awaiting an installed-app launch/wake test and explicit user confirmation. No application or AI session has been started, and no candidate has been promoted. Verify that Dock/Finder reopen remains silent and that first-run setup is reachable from the menu bar.

Protected source:

- `src-tauri/tauri.conf.json`
- `src-tauri/src/lib.rs`
- `src-tauri/src/live_window.rs`
- `src-tauri/src/wake_runtime.rs`
- `src/hooks/useLiveConversation.ts`
- `src/lib/event-scope.ts`

Required regression evidence on every change:

- Launch and repeated macOS Reopen keep both windows hidden and do not steal focus.
- Explicit menu-bar settings remain available even before onboarding is complete.
- Without a pending wake request, mounting the hidden assistant does not start a session.
- A wake request arriving during asynchronous listener registration is consumed exactly once.
- Unmounting during listener registration disposes the late listener and cannot start a session.
- Wake activation and microphone preparation keep the invisible main WebView alive until `connecting`; only the overlay is visible to the user.

### treatment-actions-and-dictated-notes

Scope: concise spoken action announcements, continuing the Treatment interface without a patient signature when AIDOO offers that option, and dictating the treatment-row note beside Procedures.

Candidate `T1` (accompanies `C6` and `W1`):

- On 2026-10-03 the user explicitly replaced the proposed critical-action confirmation requirement with a short spoken announcement, such as „Отварям Лечение“ or „Добавям процедурата“, followed by execution. No new generic confirmation gate is added.
- Routine commands use a concrete short announcement and only the necessary verified result. Requested record readings and verbatim note text are not truncated. Status keeps its protected immediate grouped write, single refresh and exact acknowledgement.
- „Диктувай/Добави/Запиши забележка“ in Treatment targets the treatment row's `note` beside Procedures, not the visit's internal note. The text is preserved verbatim.
- A pending note keeps its original patient, tooth and treatment-row binding through continuation and completion. A duplicate tool call is answered silently without replacing the capture or restarting its timer.
- A human ending phrase is recognized from the bounded current input transcript even when the model has already removed it from the tool arguments. A previous input turn's ending cannot finish a later note. The existing 10-second end-of-dictation question remains a fallback, not a generic action confirmation.
- A live note preview receives its pending patient identity and writes only the exact named note text field the user has manually focused in that patient's Treatment tab. A surrounding container is not searched for an arbitrary descendant field. If that target cannot be proved, the overlay remains the preview. Automatic opening of a specific row's note sidebar is not claimed without a reliable row identifier.
- The signature continuation uses only the exact enabled AIDOO „Продължи без подпис“ button in the bound patient tab. It does not invent an API parameter or a signature. Browser failure remains distinct from a verified clinical write, preventing an automatic duplicate write.
- Automated evidence: the final combined `T1`/`S1` candidate passes 186 Rust tests and 59 frontend tests, including immutable note targets, raw/fragmented endings, duplicate capture calls, strict preview targeting and the bounded signature-gate state machine. TypeScript, source layout, localization, release configuration, macOS dependencies, wake-word validation, website validation, production frontend build, Rust formatting, Clippy with warnings denied and diff checks pass.
- State: automated verification complete; installed-app verification and explicit user acceptance remain pending. `T1` is included in the shared 2026-10-03 test installer below. No application launch or AI session has been performed. The `C6`/`W1` installer and all previous candidates remain intact; no baseline is promoted. The preserved installer still has SHA-256 `817e5bc95bc0737de67658119c9a4e3271a274257860433c44a2d65746c243c2`.

Protected source:

- `src-tauri/src/live.rs`
- `src-tauri/src/aidoo/browser.rs`
- `src-tauri/src/aidoo/browser_surface.rs`
- `src-tauri/src/aidoo/browser_treatment.rs`
- `src-tauri/src/aidoo/commands.rs`
- `src/hooks/useLiveConversation.ts`
- `src/lib/official-note-dictation.ts`

Required regression evidence on every change:

- Routine action announcements do not introduce a generic confirmation round trip or filler; full requested readings and dictation are preserved.
- The treatment-row note remains bound to its original patient, tooth, row and full text, including duplicate calls during capture.
- A fragmented human ending completes clean tool text immediately; a stale ending does not complete a later capture.
- The 10-second fallback and its exact question remain unchanged, and the note ending cannot close the assistant session.
- Preview refuses a different patient or an unproved note field and retains the overlay transcript fallback.
- Only the exact enabled signature continuation button is pressed, at most once, and it must disappear before visible completion is reported.
- Treatment presentation waits for the bound patient/view to be ready; a missing or mismatched web area cannot be reported as completed.
- All `C6` and `W1` regression checks remain green. Actual procedure entry, visible same-tab updates and note-side-panel behavior require an installed-app test before promotion.

### individual-status-editing

Scope: replacing a selected existing dental status without losing other statuses, including grouped corrections and moves between region sets.

Candidate `S1` (accompanies `C6`, `W1` and `T1`):

- On 2026-10-03 the user explicitly requested status editing by removing the old status and entering the new one. The supplied removal request confirms ordinary PUT with exact `statuses` sets; cancellation of an entire visit's status is not part of this flow.
- „Редактирай“, „коригирай“ and „замени“ carry the old value as `replaceStatus` and the new value as `status`. A correction is never silently downgraded to an addition. Genuine ambiguity first reads the current status and asks one short question; it does not add a generic confirmation gate.
- Duplicate baseline rows for the same tooth and exact sorted region set merge their status IDs only when note, milk-tooth and observation metadata agree. Conflicting metadata rejects the draft before a write; malformed snapshots cannot compare equal or verify a write.
- All replacement sources resolve against the original baseline before mutation. A matching region set takes precedence; otherwise only a unique old-status location is accepted. An absent, ambiguous or twice-consumed source is rejected.
- Remove every selected old ID before adding the new IDs. Preserve the other statuses and existing notes, while keeping the existing AIDOO surface-flag rules and explicit note updates. A move to another region set writes both touched rows in the same PUT, including an empty source status list where needed.
- The existing stale-snapshot check, one grouped PUT, independent read-back, same-patient registered-tab refresh and exact spoken result remain in place. Transport uncertainty never repeats the write automatically, and a saved change with failed browser presentation keeps its distinct warning.
- Diagnosis evidence: before the draft/map fix, five deterministic regressions failed: duplicate-row preservation, conflicting metadata rejection, cross-region replacement, surface ambiguity reporting, and grouped swapping. The final status-edit suite passes 10/10, including duplicate-source consumption rejection and preservation of a whole milk tooth's observation flag, note and unrelated status.
- Automated evidence: the combined candidate passes 186 Rust tests, 59 frontend tests, all frontend checks, production frontend build, Rust formatting, Clippy with warnings denied and diff checks. Independent source review confirms one PUT, one independent read-back, no whole-visit DELETE, and no automatic retry.
- State: automated verification complete; installed-app same-tab rendering and explicit user acceptance remain pending. `S1` is included in the shared 2026-10-03 test installer below. No clinical request from the user's capture is replayed and no credential or patient identifier is saved in this ledger. No app launch, AI session or baseline promotion is performed; all earlier installers remain intact.

Protected source:

- `src-tauri/src/aidoo/draft.rs`
- `src-tauri/src/aidoo/types.rs`
- `src-tauri/src/aidoo/workflow.rs`
- `src-tauri/src/aidoo/protocol.rs`
- `src-tauri/src/live.rs`

Required regression evidence on every change:

- Replacing A with C on a row containing A and B yields exactly B and C; the note and applicable flags remain intact.
- Duplicate same-region rows preserve their union regardless of order, but conflicting notes or flags reject before any write.
- A unique source-to-destination region change clears only the selected old ID and adds the new ID in one PUT, followed by one independent read-back.
- An ambiguous source rejects; a matching region set resolves an otherwise repeated old status safely.
- Grouped swaps and mutually incompatible old/new combinations resolve the full group before adding anything; duplicate consumption of one old occurrence rejects.
- No individual correction emits whole-visit DELETE, repeats a clinical write, creates an extra patient tab, or bypasses presentation errors.
- The full applicable suites pass before a requested installer, and installed-app visible completion requires explicit user acceptance before promotion.

### live-startup-latency

Scope: microphone acquisition and voice-session connection time after „Хей, Айдуу“, preserving the `W1` window handoff and existing clinical commands.

Candidate `W2` (accompanies `C6`, `W1`, `T1` and `S1`):

- On 2026-10-03 the user reported startup taking more than 3–4 seconds and explicitly requested acceleration. Existing installed-app timing boundaries showed wake-to-HTTP-answer times of about 1.95, 11.95 and 8.00 seconds that day; wake-to-native-preparation was only about 0.2–0.3 seconds. Those logs aggregate microphone, ICE and session HTTP work, so they do not identify the dominant slow phase or prove complete browser readiness.
- A first/unproved microphone selection keeps the existing 7-second first attempt, 300-ms retry gap and 7-second final attempt. Only the same configured selection that returned a live audio track in the current hook lifetime uses a 1.5-second first attempt and 100-ms gap, retaining the full 7-second final attempt. Thus recovery from a stalled first attempt begins after 1.6 seconds rather than 7.3; this is a policy/synthetic result, not an installed end-to-end promise.
- Device changes, microphone-setting changes and current-operation acquisition failures invalidate warm proof. A configured selection that cannot be resolved and falls back to the default microphone stays cold. A canceled/stale acquisition or an empty/ended stream cannot establish warm proof.
- A generation/device-revision predicate is checked after the first timeout and immediately before retry. A stopped or superseded startup does not reopen the microphone. Permission rejection still has no retry; late timed-out streams are stopped.
- Live sessions share an immutable HTTPS-only HTTP client with redirects disabled and the original connect/overall timeouts. API keys, voice, SDP and session configuration remain per request. Connection reuse is eligible on subsequent calls; its actual network benefit is not yet measured. No external preconnection or idle paid session is introduced.
- Development diagnostics separate native preparation, handoff, device selection, each microphone attempt, offer construction, ICE gathering, session request, remote description and overall readiness. Native development timing further splits request building, POST and response decoding. New measurements contain only stage/outcome/attempt and elapsed milliseconds, not audio, credentials, SDP or patient data; observer failures and stale generations cannot affect startup.
- The `W1` transparent live WebView, overlay-only launch/wake behavior, full ICE gathering, session-start wait, greeting, model, voice and clinical workflows stay unchanged. The legacy 180-ms WebKit handoff remains deliberately intact: native window-command completion is not proof of browser media readiness, so removing it requires an installed readiness test.
- Automated evidence: 188 Rust tests and 75 frontend tests pass, including 19 focused microphone/timing/actual-startup-callsite tests. TypeScript, structure, localization, release configuration, macOS dependencies, wake-word validation, website validation, production frontend build, Rust formatting, strict Clippy and diff checks pass. Independent review found no remaining source-level safety blocker.
- State: source and automated verification complete; installed cold/warm wake latency, actual audio, device changes and overlay-only behavior remain unverified. `W2` is included in the shared 2026-10-03 test installer below. No application, AI session or voice/clinical API call was started. The previous installers are preserved, and this candidate is not a promoted working baseline.

Protected source:

- `src/hooks/useLiveConversation.ts`
- `src/lib/live-microphone.ts`
- `src/lib/live-startup-timing.ts`
- `src-tauri/src/live.rs`
- `src-tauri/src/live_transport.rs`

Required regression evidence on every change:

- Cold, changed or unresolved microphone selection retains its full permission/acquisition budgets; only proved warm selection takes the fast first budget.
- The final microphone attempt keeps its full budget; permission rejection is not retried and late first-attempt tracks stop.
- Stop, unmount, device change or supersession prevents a new retry and late diagnostic/cache mutation.
- Warm proof requires a current live audio track and is invalidated by device/selection changes.
- Timing observers retain no action arguments/results/errors and cannot fail or retry startup; the production UI has no development timing journal.
- HTTP-client sharing preserves HTTPS, redirects, timeouts and per-request credentials/voice/SDP without preconnecting or keeping sessions open.
- All `W1`, `T1`, `S1` and same-tab presentation checks remain green. Installed acceptance must compare cold and same-device warm starts and confirm actual listening before promotion.

## Shared test installer — 2026-10-03

- The user explicitly requested one DMG containing candidates `C6`, `W1`, `T1`, `S1` and `W2`. Exactly one new DMG was created; none of the previous installers was replaced.
- Installer: `release/test-builds/20261003-121531/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — SHA-256 `cdcd7aa58f60147e5e5c1926314ea755088c22c6aa9e7a4f4a38d34248a1c3b9`.
- Version `1.1.0`, Apple Silicon (`arm64`), macOS `13.0+`, bundle identifier `app.aidoo.whisper-lite`. The existing cache compiled the release binary in 44.17 seconds; no application or AI session was launched.
- Both the application and DMG have the expected Aidoo Developer ID signature, were accepted by Apple notarization, and have validated stapled tickets. The independent distribution audit mounted the exact final DMG read-only and passed signature, checksum, architecture, minimum macOS, permission, icon and bundled-resource checks.
- Local Gatekeeper assessments returned `accepted`, `source=Notarized Developer ID`, with `override=security disabled`. Validated Apple tickets and signatures are artifact evidence; this is not a runtime test with forced Gatekeeper enforcement.
- Final source gates: 188 Rust tests, 75 frontend tests, strict Clippy, formatting, TypeScript, source layout, localization, release configuration, macOS dependencies, wake model, website and production frontend build pass. Adjacent `build-info.json` records the executable and protected-source fingerprints.
- Acceptance pending: install this exact DMG, compare cold/warm wake startup and actual listening, verify repeated same-patient updates in the registered tab, edit a status, and exercise procedure entry without signature and treatment-row note dictation. Automated checks and notarization do not promote any candidate.
- User feedback after installation on 2026-10-03: „Край“ did not disconnect, and starting a new treatment and recording a procedure failed. Preserve this installer as evidence for those failed flows; the feedback does not reject unrelated same-tab, wake-latency or status-edit candidates. The `E1` and `T2` source candidates below are not included in this DMG.

### session-end-commands

Candidate `E1` — explicitly authorized on 2026-10-03; supersedes the historical bare-„Край“ policy, not a confirmed baseline.

- Standalone „Край“ closes after an 800-ms transcript-fragment guard. Meaningful continuation re-evaluates the full phrase: „Край на забележката“ cancels closure, while „Край на разговора“ closes. Whitespace or punctuation alone preserves the guard.
- „Затвори“, „Приключихме“ and the reviewed explicit session-ending commands close without being swallowed by pending note capture. Quoted, negated and clinical uses remain non-commands.
- Voice-requested closure of a pending note capture emits „Забележката не е записана.“ to the rendered assistant-overlay notice and diagnostic journal, then closes without a clinical write. The warning may be brief if server closure hides the overlay quickly. The existing button/tray force-close path is unchanged and does not claim this warning. Explicit note endings and the existing 10-second confirmation fallback retain their separate behavior.
- A gap of at least 1,500 ms between valid server-audio transcript intervals begins a new command/raw-input buffer; pending note text remains intact. Short gaps, overlaps, missing or malformed timings preserve the buffer. This is an approximate grouping policy, not a completed-turn signal or packet-arrival timeout. [GPT-Live transcript timing](https://developers.openai.com/api/docs/guides/live-conversations#transcript-deltas) documents the underlying event limits.
- Once closure begins, late delegated responses cannot start another clinical tool. Terminal close/error events still release resources; the existing goodbye acknowledgement is retained for inactivity closure.
- Required evidence: tests exercise the actual Live message and stop callbacks, timestamped successive utterances, fragmented note endings, punctuation-only continuation, pending note capture, rendered closing-state warning, late tool suppression, timer cleanup and exactly-once microphone/session release. Continuation arriving after the 800-ms guard expires cannot be distinguished retroactively. Installed speech recognition and warning visibility still require a user test.
- Protected source: `src/lib/assistant-command.ts`, `src/hooks/useLiveConversation.ts`, `src/Overlay.tsx`, `src-tauri/src/live.rs`. Preserve `W1`/`W2` startup and `T1` note capture when changing this flow.

### new-treatment-visit-and-procedure

Candidate `T2` — explicitly authorized by the failed-treatment report and sanitized request schemas supplied on 2026-10-03; not a confirmed baseline.

- „Ново лечение“ starts a visit, distinct from creating a tooth row. `begin_aidoo_treatment` accepts only the selected patient; the doctor comes from the authenticated session. An active unfinished/noncancelled visit is reused. Only the observed no-active-visit response permits one visit POST, followed by independent exact-ID verification; uncertainty never triggers a retry or a status-update POST.
- Patient identity must match the runtime's exact selected cursor before any network request for beginning a visit, creating a treatment row, adding a procedure, writing a diagnosis or writing its official note. An ambiguous search or a stale/different patient is rejected. Other status/schedule commands are outside this candidate's guard change.
- Tooth-body selection is local UI state plus `selectedTeeth` navigation; clicking the number is the separate milk-tooth toggle. Selecting a tooth alone does not create a visit or record a procedure.
- A new row uses a fresh-only DTO with `procedures: []`, `nzis: false`, `nhif: false` and the observed tooth/diagnosis/treatment/note/milk fields. The chosen procedure is written exactly once through the separate procedure POST, with textual price and discount, then the full composite result is independently read back before its one final same-tab refresh. Existing-row PUT payloads remain unchanged.
- A transport failure or successful but undecodable Treatment-write response may have committed: perform independent read-back without repeating the write. A new visit whose response ID is unknown remains uncertain even if an active visit appears; a row/procedure is verified only by matching the exact intended draft.
- Empty-array compatibility is a candidate assumption, not a proven production contract: the cached UI uses a blank procedure placeholder before assigning the selected catalog ID, while the user's row capture includes an item. The candidate avoids embedding a selected procedure twice; acceptance of the empty array must be verified in the installed application.
- Native diagnostics reported Treatment readiness failures while the existing patient tab visibly contained the Treatment table. Two genuine regressions reproduced the depth-16 traversal rejecting a valid deep table and missing a deeply nested signature button. A bounded breadth-first traversal replaces the depth cutoff; exhausting the node budget still fails closed. The exact patient/view, exact enabled signature button, one press and disappearance checks remain required.
- Required installed acceptance: open a patient without an active visit, say „Ново лечение“, select/dictate a tooth, add a diagnosis and a procedure, and verify one visible record in the same registered tab. Then add a procedure to an existing row, including the legitimate without-signature path. Source/mock checks are not live clinical-write evidence.
- Protected source: `src/lib/aidoo-live-tools.ts`, `src-tauri/src/live.rs`, `src-tauri/src/aidoo/{runtime,client,types,workflow,protocol,browser_treatment}.rs` and the native command/capability registrations. Preserve the `C6` tab binding and all `T1`/`S1`/`W1`/`W2` checks.
- State: source candidate only, awaiting installed verification and explicit acceptance. No new DMG, app/AI launch or real clinical write is authorized by this repair turn; all prior installers remain intact.

Combined `E1`/`T2` verification — 2026-10-03:

- 207 Rust tests and 93 frontend tests pass, including the actual Live callback with a real pending `OfficialNoteDictation`, the production traversal seam, local HTTP contract/read-back cases, and zero-network patient-selection guards. Strict Clippy, formatting, TypeScript, source layout, localization, release configuration, macOS dependencies, wake-word/website checks, production frontend build and diff checks pass. Independent source review found no remaining blocker within these two candidates; the installed-only and adjacent limitations above remain explicit.
- The preserved 2026-10-03 DMG checksum was rechecked as `cdcd7aa58f60147e5e5c1926314ea755088c22c6aa9e7a4f4a38d34248a1c3b9`. No new installer was created and no baseline was promoted.

### visible-treatment-tooth-selection

Candidate `T3` — explicitly authorized on 2026-10-03 before finishing the next requested DMG; extends `T2`, not a confirmed baseline.

- Voice selection uses `select_aidoo_treatment_tooth` for a specific FDI tooth or explicitly requested general procedures (`*`). The native command requires the exact authenticated, selected patient. Selection changes only the displayed Treatment view; it does not create a visit, save a row, write a procedure or toggle the milk-tooth flag.
- A specific tooth is carried in `selectedTeeth`. Valid milk FDI labels map to their permanent chart-body keys without pressing the number label. A general `*` expands to the deployed chart's 32 body keys: AIDOO's selection handler then displays one local general draft row, not 32 clinical writes. Literal `selectedTeeth=*` is invalid in the deployed chart.
- The existing `C6` retained patient-tab route remains authoritative. A verified Treatment write refreshes that same tab with its exact draft tooth rather than clearing the selection. Existing row/tooth mismatches reject instead of silently rebinding a procedure to a different tooth.
- Visible completion requires the exact patient/Treatment URL, normalized selection-set equality and the visible filter group with the exact teeth and paired Remove buttons. AIDOO rewrites the general query in ascending order, so textual query-order equality is not required. Stale rows or chart-number text alone cannot prove selection; missing, extra, duplicate, invalid or truncated filter evidence fails closed.
- Diagnosis: actual route-builder and Live mapping tests reproduced an empty selection and an unknown selection tool before the fix. The deployed UI source distinguishes chart-body selection from number-label milk toggling and expands all-body selection to a local `*` draft.
- Live UI evidence: in the already-open DEMO tab, choosing 26 exposed its selected chart body, filter and local row; the exact 32-body query exposed 32 selected bodies, all filter chips and one local `*` draft. AIDOO's native accessibility tree exposed the same filter chips. The original 18 selection was restored. No diagnosis/procedure/note/save action was performed, and no app or AI session was launched. This is deployed-browser contract evidence, not an installed-candidate clinical-write test.
- Required acceptance: install the exact next DMG; select a permanent tooth, a milk tooth and general procedures by voice; verify their visible state in the same patient tab; add a procedure/diagnosis/note to the intended row and verify its tooth remains selected after refresh. Confirm patient A → B → A, no repeated tab creation, note endings and session closure. Only explicit user acceptance permits promotion.
- Protected source: `src/lib/aidoo-live-tools.ts`, `src-tauri/src/live.rs`, `src-tauri/src/aidoo/{mod,protocol,presentation,browser_treatment}.rs`, their tests and native command/capability registrations. Preserve all `C6`, `E1`, `T1`, `S1`, `W1` and `W2` evidence.
- Automated evidence: 219 Rust tests and 94 frontend tests pass, including the route/visible-filter guards, exact Live/native mapping, zero-network patient-selection guards and wrong existing-row tooth rejection. Strict Clippy, formatting, TypeScript, source layout, localization, release configuration, macOS dependencies, wake-word/website checks, production frontend build, diff and debug-marker checks pass. Independent source review found no remaining safety blocker. The subsequent presentation also runs for an Ok non-verified workflow outcome; it performs no clinical write, and the verification failure remains the spoken result.
- State: source and combined verification complete; included in the single signed/notarized installer below. Previous installers remain intact; no candidate or working baseline has been promoted. Installed-candidate voice/clinical acceptance remains pending.

## Shared signed installer — 2026-10-03, T3

- The user explicitly authorized visible tooth selection before finishing one new DMG. Exactly one new DMG was created from the production release configuration, containing `C6`, `W1`, `T1`, `S1`, `W2`, `E1`, `T2` and `T3`; it is a user-unconfirmed candidate, not a promoted production baseline.
- Installer: `release/test-builds/20261003-144110/AIDOO Whisper Lite_1.1.0_aarch64.dmg` — SHA-256 `2a9717ba22b6db01158924624ab5d9b7214757dccc9d577a8918d8c26a8eec9b`.
- Version `1.1.0`, Apple Silicon (`arm64`), macOS `13.0+`, bundle `app.aidoo.whisper-lite`, expected Aidoo Developer ID/Team `4KKVT2TUUA`, hardened runtime. The incremental release compilation took 42.74 seconds. No application or AI session was launched and no clinical write was performed.
- App and DMG notarization were accepted by Apple and their stapled tickets validated. The independent audit mounted the exact final DMG read-only and passed signatures, checksum, architecture, version, minimum macOS, microphone descriptions, resources/icon and entitlements/no Apple Events checks. Mounted executable SHA-256 `a96f0a7af0ef50bffb447be14b1f04d56327f93ea24e73e8873468536075408f` exactly matched the newly built app.
- Gatekeeper assessments returned `accepted`, `source=Notarized Developer ID`, with `override=security disabled`. Apple ticket/signature validation is confirmed; enforced Gatekeeper runtime acceptance was not tested on this host.
- Build boundary: HEAD `ec6f3d522ed3c0e2e39405676e6b33f3a9a2d838`, dirty tree, 171 source files, fingerprint `a00af0141b956f62674681bfc26af5912133a593bfe192edebeb6a7ad7fc53b4` before building and unchanged after packaging, before this artifact-ledger append. Adjacent `build-info.json` records exact source hashes and acceptance limitations. The later ledger append is documentation only and is not a change to the compiled candidate.
- Final gates: 219 Rust tests, 94 frontend tests, strict Clippy, formatting, TypeScript, layout, localization, release configuration, macOS dependencies, wake-word/website, production frontend build, diff/debug checks and independent source review pass. Live browser screenshots beside the metadata demonstrate the deployed 26 and general `*` selection contract without recording clinical data.
- Previous 20261003-121531 installer remains unchanged at SHA-256 `cdcd7aa58f60147e5e5c1926314ea755088c22c6aa9e7a4f4a38d34248a1c3b9`; all earlier installers remain preserved.
- Acceptance pending: the installed candidate's voice, same-tab selection/refresh, new visit and diagnosis/procedure/note writes, the unproven fresh-row empty-array contract, wake latency and `E1` closure behavior. Promote only after an explicit statement from the user that this exact candidate works, then capture the source baseline through the promotion procedure above.

### treatment-save-contract-and-readiness

Candidate `T4` — authorized by the failed Treatment-save report on 2026-10-03; supersedes the `T2` empty-array assumption and extends `T3`, not a confirmed baseline.

- Installed evidence: the running application exactly matches the signed T3 executable hash recorded above. Its persistent journal records intermittent retained-tab Treatment readiness timeouts, not clinical HTTP outcomes; it cannot prove a visit/row/procedure POST failed. Read-only browser inspection found the correct loaded Treatment table. No clinical write, app/AI launch or new DMG was performed during diagnosis.
- Fresh procedure rows now follow the user's sanitized production capture: one first chosen procedure placeholder with textual zero price/discount, then each intended procedure POST exactly once and one final independent read-back. Existing-row PUT remains unchanged. The captured contract proves one placeholder; grouped fresh procedures and fresh diagnosis-only/note-only rows still require installed acceptance.
- Verification carries the exact successful created-row ID and any consistent nonempty `treatmentId` returned by successful procedure responses. It checks exact diagnosis, note, status and milk flag, plus nonempty joined procedure ID and intended price/discount (numeric and textual amounts normalize). Another new row or an unfinalized placeholder cannot prove the intended write. Unknown row IDs, partial/rejected writes and conflicting responses remain uncertain with no automatic write retry.
- Treatment row lookup canonicalizes the spoken FDI body key and requires exact permanent/milk identity in both implicit and explicit row selection. Raw milk FDI is retained for new-row normalization. A matching existing milk row is reused; a permanent row never substitutes for it or vice versa. Multiple matching rows require clarification.
- A ready Treatment page completes after three consecutive exact-route/header/filter observations instead of waiting for a fixed deadline. Slow hydration has an eight-second bound. Full bounded AX scanning still gives the exact enabled „Продължи без подпис“ button precedence, presses it once, and requires disappearance and stable readiness. Truncated or wrong-patient/selection evidence still fails closed. The currently loaded production `TreatmentTable` awaits its signature decision before rendering headers and its warning dialog; future modal-timing changes require renewed browser evidence.
- Persistent support diagnostics contain only whitelisted Treatment stage/error kind/outcome/visibility and bounded readiness predicates/counts. Patient URLs, names, tooth values, notes, payloads and credentials are excluded. The visible development journal remains development-only.
- The user clarified the commands „Можем ли да запишем лечение?“ and „Запиши зъб Х и процедура У“. The backend instruction routes the first through the visit tool and the second through the existing-row-or-missing-row procedure tool, without an extra human confirmation. A following write requires a verified ready visit and `visibleInBrowser=true`; explicit new-row intent remains separate. [Official Live delegation guidance](https://developers.openai.com/api/docs/guides/live-delegation) keeps backend tool rules separate from speech, with application-side authority checks. Model/voice/endpoints, `C6` tab binding, `T1` notes, `S1` statuses and `W1`/`W2` wake/microphone flows are unchanged.
- Regression evidence: genuine RED cases covered the fresh payload, placeholder false verification, returned treatment ID, wrong created-row identity, wrong milk flag, milk/permanent row lookup and ready-page deadline behavior. The serialized-session wording test is a prompt-contract check, not proof of model behavior. Final checks pass: 236 Rust tests, 94 frontend tests, strict all-target/all-feature Clippy, formatting, TypeScript, layout/localization/release/macOS/wake/website validation, production frontend build and diff checks. Independent review found no remaining source blocker.
- State: source candidate only. The preserved T3 DMG checksum was rechecked as `2a9717ba22b6db01158924624ab5d9b7214757dccc9d577a8918d8c26a8eec9b`; it does not contain `T4`. A new signed installer and a controlled installed test remain required before acceptance. Test natural commands, new/missing/existing/general/milk rows, without-signature entry, procedure/diagnosis/note read-back, one visible same-tab update and failure/uncertainty without duplicates. Promote only after explicit user confirmation and the baseline capture procedure.
