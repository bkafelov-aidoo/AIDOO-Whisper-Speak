# AIDOO Kontrol contract unknowns

The deployed frontend contract now supports an isolated pilot client and voice tools. These remaining questions block a clinical production release, not local mock validation.

## Authentication and tenancy

- Sanitized login request and response shapes, token lifetime, expiration behavior, and logout behavior. The session endpoint and `X-Auth-Token` header name are observed.
- How the clinic UUID returned/used by the authenticated application maps to the `demo` route slug.
- Exact behavior after an expired session and whether one safe reauthentication attempt is supported.
- Production hostname and whether it differs from the development test host.

## Patients and visits

- Server-side handling below four characters, pagination, zero-result and multiple-result behavior, and the `nextAppointment` object schema. The endpoint, one-result schema, and observed four-character client minimum are recorded.
- Patient response schema and the source of the opaque patient identifier. The canonical medical-record browser route is observed.
- Business rule for when a visit becomes active or finished, and whether the dedicated active-visit endpoint is always authoritative. The read endpoint, success shape, and no-active-visit `400` are observed.
- Whether a live НЗОК status visit requires compliance preflights beyond the deployed `has-status-checks` contract. The signer/NHIF preflight, clinic capability flag, and same visit POST for both choices are source-observed; a controlled live НЗОК write is still pending.
- Whether a separate canonical visit route exists. The status view is currently represented by `mode=status` in the patient medical-record route.

## Dental status catalog

- Whether catalog identifiers remain stable when localized labels change. The deployed frontend uses `GET /statuses` and opaque IDs.
- Server validation rules for incompatible status combinations beyond the currently deployed client filtering.
- A controlled confirmation of every region mapping in a successful write. The deployed client enumerates `MESIAL`, `DISTAL`, `OCCLUSAL`, `VESTIBULAR`, `LINGUAL`, `PALATAL`, `CERVICAL_LINGUAL`, `CERVICAL_VESTIBULAR`, and `CERVICAL_PALATAL`.

## Read, write, and verification

- Whether the observed per-visit teeth-status read is the canonical complete-status read in every workflow, including when no visit is active.
- Semantics of the editable `GET .../teeth-status?visitId={visitId}&isNzok=false` response, especially why empty teeth have allocated record identifiers.
- Failure and cancellation behavior of the local signer/NHIF preflight. Non-NZОК clinics bypass it; NZOK clinics require a successful preflight before the funding choice can be offered.
- A retained browser Network capture of a successful surface add, replacement, and multiple change. The deployed frontend source establishes the request contract, but a real controlled write is still required before clinical release.
- The 2026-10-03 user-supplied removal request confirms ordinary PUT with exact, possibly empty `statuses` sets. Its independent response/read-back was not supplied. In particular, confirm whether `/teeth-status/visits/{visitId}` returns an emptied surface row or omits it; the current verifier conservatively requires the expected row and never retries an unverified write.
- Whether the server has its own optimistic-concurrency field. The Lite pilot compensates by comparing the full editable snapshot immediately before writing.
- Idempotency support, if any. No idempotency behavior will be inferred.
- Validation error schema and partial-success behavior.
- Whether partial success is possible when `teethStatus` contains multiple records. The Lite client treats any read-back mismatch as uncertain.

## Diagnoses, procedures, and official notes

- Controlled Network evidence for diagnosis selection, procedure creation and the treatment-row `note`. The fixed routes and payloads are extracted from the deployed frontend and covered by local contract tests.
- The user supplied production visit, fresh-row and procedure request schemas on 2026-10-03, but no independent read-back for that composite operation. After failed installed Treatment saving, `T4` follows the captured single chosen-procedure placeholder with zero textual amounts, then the separate finalized procedure POST. End-to-end server acceptance, grouped procedures and fresh diagnosis-only/note-only rows remain unverified; local contract tests are not live write evidence.
- Whether procedure price currency must be sent explicitly when the clinic catalog returns a currency-specific price. The deployed active-treatment UI posts the selected procedure object with `procedureId`, `price` and `discount`.
- Server behavior for duplicate procedure IDs, incompatible diagnosis/treatment combinations, and rows created with only a note.
- Whether one treatment row can be selected reliably from speech when multiple active rows use the same tooth. The assistant currently requires the opaque existing row ID after a disambiguating read; it must not guess.
- Transactionality across the treatment write and one or more procedure writes. The Lite client does not retry and reports a partial/uncertain result when read-back does not confirm the complete intended state.
- Installed-app Accessibility exposure and timing of the exact „Продължи без подпис“ button, followed by visible procedure/note updates in the registered patient tab. Source extraction and bounded state-machine tests do not prove the live browser behavior.
- Reliable automatic opening of a specific treatment row's note sidebar, and React input-event behavior for live preview. Until established, preview requires a manually focused exact note field for the pending patient and otherwise remains in the overlay; the final note still uses the API write and independent verification.

## Schedule

- Controlled live evidence for a successful appointment POST and its independent search read-back. The deployed frontend route and payload are extracted and locally contract-tested, but implementation work did not create a real appointment.
- Server-side conflict semantics when two clients book the same doctor or room concurrently. The Lite client performs an immediate preflight but still treats a rejected or unverified write conservatively.
- Clinic-specific rules beyond doctor work intervals, room occupancy and 15-minute granularity, such as holidays, appointment-type duration rules or hidden buffers.
- Whether an idempotency key is supported for appointment creation. The Lite client does not infer one and never retries an ambiguous POST.

## Browser training exit criteria

The production-readiness session is complete when sanitized evidence covers login expiration, catalogs, private and НЗОК status visits, tooth add, surface add, replacement, multiple change, diagnosis, procedure, official treatment note, server validation failure, ambiguous or partial write outcome, and independent read-back. The fixed-host client, strict semantic tools, confirmation flow, and mock contract tests are implemented on the isolated pilot branch; they do not by themselves prove the live clinical workflow.
