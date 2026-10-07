# AIDOO Kontrol browser discovery log

This file records only facts observed in the browser session against the test clinic. It must not contain passwords, session tokens, cookies, real patient data, full names, or screenshots with identifying data.

## Test environment

- Entry point: `https://aidoo-web.on.dev-craft.tech/clinics/demo/login`
- Environment: development test clinic
- Web host: `aidoo-web.on.dev-craft.tech`
- API host: `aidoo-platform.on.dev-craft.tech`
- Status: controlled write discovery in progress against the designated test record
- Code rule: no endpoint, payload, identifier, or retry behavior becomes production code before it is recorded and reproduced here.

## Session procedure

For each step, use a designated test patient and keep the browser Network panel recording. Capture the request immediately after one controlled UI action, then repeat the relevant read request to verify the result.

1. Sign in and record the authentication exchange with all secret values removed.
2. Search for one test patient using the smallest accepted query.
3. Open the patient and the active visit; record the canonical browser routes.
4. Load the complete dental status and the status/surface catalog.
5. Add one tooth-level status, then verify it with the normal read request.
6. Add one surface-level status, then verify it.
7. Replace one existing status, then verify it.
8. Apply a controlled multiple-status change, then verify every item.
9. Observe removal of one selected status to establish replacement semantics. Deletion of an entire visit's status remains outside the Lite voice flow.

## Evidence template

Copy this block for each observed action. Replace credentials, cookies, tokens, names, birth dates, free text, and patient identifiers with stable placeholders such as `<TOKEN>` and `<TEST_PATIENT_ID>`.

```text
Action:
Observed at:
Browser route before action:
HTTP method:
Full URL and query:
Request headers (sanitised):
Request body (sanitised):
Response status:
Response body/schema (sanitised):
Follow-up read request:
Verified UI result:
Stable identifiers observed:
Ambiguities / follow-up:
```

## Observed contracts

### Static frontend contract extraction (2026-09-16)

The deployed test frontend bundles were inspected after the browser debugger stopped retaining requests. This is source-level evidence from the same deployed environment, distinct from a successful controlled write. It established the following client contract:

- `POST /clinics/{clinicSlug}/sessions` with `{ email, password }`; response includes `sessionId`, `user.id`, and `user.clinic.id`.
- `GET /statuses` returns the current status catalog.
- `POST /clinics/{clinicId}/patients/{patientId}/teeth-status?visitId={visitId}&isNzok={boolean}` creates the visit's status update.
- `PUT /clinics/{clinicId}/patients/{patientId}/teeth-status?visitId={visitId}` writes `{ teethStatus: ToothStatusWrite[] }`.
- `GET /clinics/{clinicId}/patients/{patientId}/teeth-status?visitId={visitId}&isNzok={boolean}` reads the editable snapshot used for stale-draft detection.
- `GET /clinics/{clinicId}/patients/{patientId}/teeth-status/visits/{visitId}` is the independent post-write read-back.
- `POST /clinics/{clinicId}/patients/{patientId}/visits` with `{ doctorId }` creates the visit for both funding choices. The following status-update POST carries `isNzok=true` for НЗОК or `false` for private reception.
- `GET /clinics/{clinicId}/diagnoses` returns the diagnosis catalog.
- `GET /clinics/{clinicId}/procedures/prices?sortBy=name&direction=ASC&currency={clinicCurrency}` returns procedures and their current prices; the currency comes from the authenticated clinic.
- `GET /clinics/{clinicId}/patients/{patientId}/visits/{visitId}/treatments` reads the treatment rows used for stale-draft and post-write verification.
- `POST /clinics/{clinicId}/patients/{patientId}/visits/{visitId}/treatments` creates a row; `PUT` on the same route with `/{treatmentId}` updates diagnosis, treatment and `note`.
- `POST /clinics/{clinicId}/patients/{patientId}/treatments/{treatmentId}/procedures` adds a procedure with `{ procedureId, price, discount }`.
- The “Забележки” control beside procedures writes the treatment row's `note`; it is distinct from the visit's internal note.
- The normal UI sends only affected records in `teethStatus`. It removes `id`, `timestamp`, `generatedByProcedure`, and UI-only `fake` before writing.
- Surface records are keyed by the exact sorted `regions` set. Their status IDs are merged for that region set and they force `isMilkTooth=false` and `forObservation=false`.
- `PUT .../teeth-status/overwrite` belongs to the NZIS import/overwrite flow and is not the ordinary correction endpoint.

The Lite client implements these fixed routes, exact write shapes, a pre-write snapshot comparison, and separate post-write read-backs. A transport timeout triggers one read-back and never an automatic second write. A composite diagnosis/procedure/note action reports partial or uncertain state if one write succeeds and a later write fails.

### Production NZOK and milk-tooth contract extraction (2026-09-17)

The deployed production frontend bundles were inspected without retaining any patient values, session tokens, signed XML or NHIF response data.

- Clinic capability comes from authenticated `user.clinic.worksWithNzok`.
- Before offering an NZOK status visit, the frontend gets `GET /clinics/{clinicId}/nzok-checks/patients/{patientId}/nzis-search-xml`, signs that XML through `POST http://localhost:4567/sign`, gets four-year NHIF data through `POST http://localhost:4567/nhif/query-four-year-data`, and sends `{ signedNzisXml, nzokData }` to `POST /clinics/{clinicId}/nzok-checks/has-status-checks`.
- The funding-choice dialog is shown only when the clinic works with NZOK and `has-status-checks` returns `true`. Otherwise the frontend starts a private status visit directly.
- A milk tooth is not sent as 51–85. The write retains the base chart tooth 11–45 and sets `isMilkTooth=true`; the UI displays it by adding 40 to the base number. Clicking the tooth-number label toggles this flag. Positions 6–8 cannot be toggled to milk teeth.
- `forObservation` belongs to the whole-tooth status row. Surface rows force both `isMilkTooth=false` and `forObservation=false`.
- The status write already accepts `teethStatus[]`; several dictated changes are therefore grouped into one PUT, one read-back and one visible refresh.
- A new treatment row can carry its diagnosis in the row POST and then receive one or more procedure POSTs. The voice operation performs the full composite sequence before its single visible refresh and verifies the resulting row once.

### Individual-status removal and replacement (2026-10-03)

Source-level evidence from production `MedicalRecordPage.131f6f40.js` (SHA-256 `a9ac84976f06cc681391a1a980d9718698a41c4aef96359d9141804f17c30234`), distinct from a controlled successful Network write:

- Removing one status collects the rows matching the tooth and exact sorted region set, unions their status IDs, removes only the selected ID, and sends the resulting exact set through ordinary `PUT .../teeth-status?visitId={visitId}` with `{ teethStatus: [row] }`. Removing the last status sends `statuses: []`.
- Adding a status unions the existing IDs with the new ID before that same PUT. Consequently, a replacement can remove the old ID and insert the new ID in one exact-set write; separate delete/add requests are not required.
- The bundle's `DELETE .../teeth-status?visitId={visitId}` has no per-tooth body and belongs to cancellation of the visit's whole status. It is not used for replacing one selected status.
- The user supplied a Network screenshot showing the `teeth-status?visitId=...` route and a successful `200` response, then a removal request copied as cURL. The supplied request explicitly uses PUT and sends two rows for one tooth: a whole-tooth row with `regions: []` and a surface row with two regions, both with `statuses: []`, false milk/observation flags and `note: null`. It also includes read-response metadata (`id`, `timestamp`, `generatedByProcedure`); the source-extracted ordinary write shape omits those fields. This corroborates exact-set removal, not an HTTP DELETE. The request was not replayed, and no token or patient/visit/clinic identifiers are retained here; no response/read-back for that exact request was supplied.
- Replacement preserves unrelated statuses and the affected row's metadata. A move to another region set touches both the source and destination rows in the same PUT. Ambiguous old-status locations or conflicting duplicate-row metadata must be rejected before any write, rather than guessed.
- All replacements in one dictated group resolve against the initial snapshot. Old IDs are removed before new IDs are added, then one independent read-back and one visible same-tab refresh verify the complete group. This is locally contract-tested behavior, not proof of installed-app clinical completion.

### Treatment signature continuation and dictated row note (2026-10-03)

Source-level evidence from the production `MedicalRecordPage.131f6f40.js` bundle, not a controlled clinical write:

- The exact „Продължи без подпис“ UI button calls the `electronic-signature` store's `skipSignature()` and closes the dialog. That action sets the local `skippedSignature` flag; it does not send a request or add a treatment/procedure API parameter.
- The continuation is therefore a guarded UI action in the bound patient tab, not a new endpoint, forged signature or write payload option.
- The Treatment table has distinct „Процедури“ and „Забележки“ columns. Its „Добави забележка“ control opens the „Забележка“ side panel and edits the treatment row's `note`, distinct from a visit's internal note.
- Final dictation remains a verified treatment-row API write. A visual transcript is only a preview and must not also submit the side-panel Save action.
- Automatic sidebar opening and full visible procedure/note completion are not established by this static contract. They require exact row targeting and an installed-app test; an unproved preview target keeps the overlay fallback.

### Active treatment visit and procedure entry (2026-10-03)

The user supplied production cURL captures for the following schemas; none was replayed, and credentials and patient/clinic/visit identifiers are not retained:

- „Ново лечение“ is `POST .../patients/{patientId}/visits` with only `{ doctorId }`. This creates the active visit, not a tooth row. The selected doctor is the authenticated session's doctor.
- The fresh-row `POST .../visits/{visitId}/treatments` capture carries `tooth`, nullable `diagnosisId`, `treatmentId`, `note`, `procedures`, `nzis`, `nhif` and `isMilkTooth`. Its procedure item contains textual `price` and `discount`.
- A procedure on an existing row is `POST .../treatments/{rowId}/procedures` with `{ procedureId, price, discount }`, again with textual amounts.

The cached paired production frontend chunks further establish ordering: a tooth-body click changes local selection and the router query without HTTP; the number label separately toggles milk-tooth status. When selecting a procedure on an unsaved row, the UI first creates the row containing a blank placeholder, then assigns the catalog ID/price and sends the separate procedure POST, finally re-reading the rows. The captures alone do not prove that embedding a selected procedure in the row POST substitutes for the separate procedure request.

The historical `T2` candidate instead sent an empty procedure array. The user reported failed Treatment saving with that installed candidate. `T4` now follows the supplied production capture: the fresh row contains the first chosen procedure ID with textual zero price/discount, followed by each intended separate procedure POST exactly once and independent read-back. The capture proves a single placeholder, not a grouped fresh-row array. Diagnosis-only/note-only fresh rows still have no procedure placeholder; their production acceptance remains unverified. Existing-row update payloads do not acquire fresh-row flags or embedded procedures.

The procedure response contains `procedure` and an optional `treatmentId`; the deployed UI assigns that returned treatment ID to the row. Verification must retain that exact ID, the known created row ID, milk/permanent identity and status, and finalized procedure IDs with intended price/discount. A placeholder alone is not proof that a rejected procedure POST succeeded. An unknown created-row ID after an ambiguous/undecodable response remains uncertain and is not retried.

Current loaded production assets were rechecked read-only on 2026-10-03 as `index.a60a25bb.js` and `MedicalRecordPage.131f6f40.js`. `TreatmentTable` has async setup: the signature availability check resolves and sets the warning-dialog state before its table headers render. The full AX scan gives that exact enabled warning button precedence over header readiness. This source contract supports three stable ready polls without waiting for the entire hydration timeout; a future change to modal timing needs renewed UI evidence. No clinical write was performed during this inspection.

### Schedule read and write contract (2026-09-17)

The deployed schedule page was reloaded with the Network domain enabled. Only request paths, payload schemas and response schemas were retained; no patient, user, clinic or appointment values were copied.

- Browser route: `/clinics/{clinicSlug}/schedule`.
- The route accepts `mode=doctors`, `active-date=YYYY-M-D`, and JSON-encoded `selected-doctors`. AIDOO Control uses these parameters plus `aidooControlSync` to display the exact day and doctor after both slot discovery and booking.
- `GET /clinics/{clinicId}/users` returns doctors and `schedules[]` with `treatmentRoomId`, uppercase `weekday`, `startTime`, `endTime` and `repetitionType`.
- `GET /clinics/{clinicId}/treatment-rooms` returns the room identifiers and names.
- `POST /clinics/{clinicId}/appointments/search` accepts `{ doctorIds: string[] | null, treatmentRoomIds: string[] | null, fromDate: YYYY-MM-DD, toDate: YYYY-MM-DD }`.
- Appointment search results contain `id`, `appointmentType`, `doctor.id`, `treatmentRoomId`, UTC `startTime`/`endTime`, and an optional `patientAppointment.patientId`. Embedded names, phones and clinic metadata are deliberately not modelled.
- `POST /clinics/{clinicId}/appointments` creates a patient appointment with `appointmentType`, `doctorId`, `treatmentRoomId`, UTC `startTime`/`endTime`, and `patientAppointment` containing the selected patient's current identity fields, `status=SCHEDULED`, and `dentalTechnologyReadiness=NOT_SET`.
- The frontend source also exposes GET/PUT/DELETE routes for an existing appointment. The Lite voice flow does not expose modification or deletion.

Slot discovery intersects the selected doctor's own work intervals with both doctor and room occupancy in 15-minute increments. An explicit date is searched only on that date; an omitted date searches from today through 30 days. The proposed slot is memory-only. Booking requires its opaque slot ID, checks the same day again immediately before the write, performs one POST, reads the day again, and refreshes the visible schedule. A transport failure triggers read-back only and never a second POST.

All identifiers below are route placeholders. No observed patient, visit, clinic, user, or session identifier is stored in this file.

### Login and session creation

- Browser entry route: `GET /clinics/{clinicSlug}/login` on the web host.
- A successful sign-in issued `POST https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicSlug}/sessions` and returned `200`.
- Authenticated API calls include an opaque token in the `X-Auth-Token` request header.
- The token value, login payload, cookies, and account data were deliberately not captured.
- Still unobserved: the sanitized login request/response schema, token lifetime, expiration behavior, and logout contract.

### Patient medical-record route

- Canonical patient-record route:
  `/clinics/{clinicSlug}/medical-record?patientid={patientId}&tab=record&mode=treatment`
- Status-editing route:
  `/clinics/{clinicSlug}/medical-record?patientid={patientId}&tab=record&mode=status&selectedTeeth=&triggerNzokChecksProp=true`
- Switching between `mode=treatment` and `mode=status` changed the visible table without issuing another API request once the page data was loaded.
- A clean reload of `mode=status` issued the patient, visits, and teeth-status reads described below.
- AIDOO Control opens the canonical treatment route when a patient is selected. It appends an `aidooControlSync` cache-busting query value to both treatment and status navigation. Direct network navigation to a `/clinics/...` route currently returns `403`; successful deep-link loading depends on AIDOO's Workbox navigation response. The deployed Workbox cache has also been observed with a stale `index.html` body under the current revision key, pointing at a removed hashed bundle. Before navigating, AIDOO Control therefore fetches only `/index.html` with browser-cache reload semantics, replaces only the matching `index.html` entry in AIDOO's own precache, and then navigates to the canonical deep link. It does not clear cookies, local storage, clinical data, or unrelated browser caches. The treatment route does not include the status-only `selectedTeeth` or `triggerNzokChecksProp` parameters.

### Patient search

- Search is opened from the global patient-search control without leaving the current browser route.
- The UI did not issue a request for test queries of one, two, or three characters. It issued the request after a four-character query was submitted, so the observed client-side minimum is four characters.
- Method and route:
  `GET https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/search?query={urlEncodedQuery}`
- Response status: `200` with `Content-Type: application/json`.
- Request body: none observed.
- Response root: JSON array.
- Each observed result has this outer shape:

```text
{
  patient: {
    id: string,
    firstName: string,
    middleName: string | null,
    lastName: string,
    birthdate: ISO-8601 date,
    email: string | null,
    mobilePhone: string | null,
    city: string | null,
    country: string | null,
    county: string | null,
    ekatte: string | null,
    street: string | null,
    streetNumber: string | null,
    neighbourhood: string | null,
    block: string | null,
    entrance: string | null,
    floor: string | null,
    apartment: string | null,
    allergies: unknown | null,
    diseases: unknown | null,
    medicalHistory: unknown | null,
    comingFrom: unknown | null,
    discount: number | null,
    identifier: string | null,
    identifierType: string | null,
    publicHealthInsured: boolean | null,
    publicHealthInsuredLastUpdated: unknown | null,
    personalInsuranceNumber: string | null,
    rzokNumber: string | null,
    healthRegion: string | null,
    pensioner: boolean,
    institutionalized: boolean,
    mentalIllness: boolean,
    gender: string,
    pending: boolean,
    balance: number | null,
    balanceCurrency: string | null
  },
  nextAppointment: object | null
}
```

- A returned result links to the canonical medical-record route using `patient.id` as the `patientid` query value.
- No pagination parameter was sent for this one-result test. Zero-result, multiple-result, pagination, and `nextAppointment` object schemas remain unobserved.
- Search results include protected health and identity data. AIDOO Control must only retain the minimum fields needed to disambiguate a patient and must never log response values.

### Patient read

- Method and route observed during a clean status-page reload:
  `GET https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/{patientId}`
- Response status: `200` with `Content-Type: application/json`.
- The patient response schema still needs a dedicated sanitized inspection. No patient fields or values are recorded yet.

### Visit list read

- Method and route:
  `GET https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/{patientId}/visits`
- Response status: `200` with `Content-Type: application/json`.
- Request body: none observed.
- Authentication header name: `X-Auth-Token`; its value is secret and is not recorded.
- Response root: JSON array.
- Fields observed on each visit include:
  - `id: string`
  - `doctor: object`
  - `price: number`
  - `priceCurrency: string`
  - `payments: array`
  - `note: string | null`
  - `timestamp: ISO-8601 string`
  - `createdStatusUpdate: boolean`
  - `isFinished: boolean`
  - `cancelled: boolean`
  - `isSentToNzis: boolean`
  - `cancelSentToNzis: boolean`
  - `nzokCompliancePassed: boolean | null`
  - `hasDentalTechnologyWorkOrder: boolean`
- The response embeds a large doctor/clinic object. AIDOO Control should model only fields required by its workflow and must not log the embedded personal or clinic data.
- The UI subsequently used one visit `id` as `{visitId}` in the teeth-status read. The rule by which that visit is selected has not yet been established.

### Active visit read

- Method and route:
  `GET https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/{patientId}/visits/active`
- With an active visit, the endpoint returned `200` and one visit object. The observed visit fields match the item shape from the visit-list response, including `id`, `doctor`, `timestamp`, `createdStatusUpdate`, `isFinished`, and `cancelled`.
- The observed active visit had `isFinished: false` and its `id` was used as the `visitId` for the editable teeth-status read.
- With no active visit, the same `GET` returned `400` with this sanitized shape:

```text
{
  error: "No active visit found for patient with id: <PATIENT_ID>",
  details: null
}
```

- The error embeds the patient identifier in its message. AIDOO Control must translate it to a Bulgarian user-facing message and must not log the original message.

### Complete teeth status for one visit

- Method and route:
  `GET https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/{patientId}/teeth-status/visits/{visitId}`
- Response status: `200` with `Content-Type: application/json`.
- Request body: none observed.
- Authentication header name: `X-Auth-Token`; its value is secret and is not recorded.
- Response root schema:

```text
{
  visitTeethStatus: Array<{
    currentToothStatus: ToothStatus,
    previousToothStatus: ToothStatus | null
  }>
}

ToothStatus = {
  id: string | null,
  tooth: string,
  statuses: string[],
  isMilkTooth: boolean,
  forObservation: boolean,
  regions: string[],
  timestamp: ISO-8601 string | null,
  note: string | null,
  generatedByProcedure: boolean
}
```

- Teeth without a current recorded status are still present. They use `id: null`, an empty `statuses` array, an empty `regions` array, and `timestamp: null`.
- `statuses` contains opaque status identifiers. One tooth may contain more than one status identifier.
- `regions` contains uppercase surface identifiers. Values observed in the read response include `OCCLUSAL` and `MESIAL`; this is evidence of values in use, not a complete allowed-value catalog.
- `previousToothStatus` either has the same `ToothStatus` shape or is `null`.
- This read was observed once after a clean reload. It must be repeated after a controlled write before it can serve as write verification evidence.

### Editable teeth status for the active visit

- Opening the existing UI flow for a new status caused the UI to resolve the active visit and then issue:
  `GET https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/{patientId}/teeth-status?visitId={visitId}&isNzok=false`
- Response status: `200` with `Content-Type: application/json`.
- Response root schema:

```text
{
  teethStatus: ToothStatus[]
}
```

- `ToothStatus` has the same field shape documented for the per-visit complete-status read.
- In this editable response, every tooth entry observed had a non-null `id` and timestamp, including teeth whose `statuses` and `regions` arrays were empty. This differs from the complete-status history response, where empty teeth may have `id: null` and `timestamp: null`.
- The UI also issued reads to `/nzok-checks/...` and calls to a local signer/NHIF helper on `localhost:4567` while opening this flow. These are part of the current web application's compliance flow and need separate study before AIDOO Control decides whether it should open or reproduce this UI behavior.
- Opening an already selected status dropdown did not issue a catalog API request. The labels appear to be available in the client application.
- The dropdown exposed these non-surface labels:
  `Мостоносител`, `Мостово тяло`, `Протеза`, `Свръхброен зъб`, `Липсващ зъб`, and `Шина / Адхезивен мост`.
- It exposed four surface-aware status families: `Обтурация`, `Кариес`, `Дефект на възстановяване`, and `Некариозна лезия`.
- Each surface-aware family was displayed with six localized surface choices:
  `Оклузално / Инцизално / Куспидално`, `Медиално`, `Дистално`, `Букално / Лабиално`, `Лингвално / Палатинално`, and `Цервикално`.
- Observed response enum values now include `OCCLUSAL`, `MESIAL`, and `DISTAL`. The exact enum mapping for the other localized surface choices remains unobserved.
- Existing UI rows demonstrated one status, multiple statuses, one surface, and multiple surfaces on a tooth. This establishes display capability only; allowed write combinations still require controlled write evidence.
- Closing the empty editor issued only `GET` refreshes in the captured network log. No status write was observed and no medical record value was changed.

### Creating a private visit from the status flow

- When no visit was active, opening `Нов статус` first asked how the procedure would be funded: `НЗОК` or `Частен прием`.
- Choosing `Частен прием` issued:
  `POST https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/{patientId}/visits`
- Response status: `200` with `Content-Type: application/json`.
- Authentication header name: `X-Auth-Token`; its value is secret and is not recorded.
- Sanitized request body:

```text
{
  doctorId: string
}
```

- The response is a visit object with the same shape as the visit-list item. The newly created private visit had `createdStatusUpdate: false`, `isFinished: false`, `nzokCompliancePassed: null`, `payments: null`, and `price: null`.
- The returned `id` was immediately used as `{visitId}` in `GET .../teeth-status?visitId={visitId}&isNzok=false`, after which the status editor became available.
- Product requirement: when a new visit is required, AIDOO Control asks once whether it is `НЗОК` or `Частен прием` and treats the unambiguous answer as the choice. It does not add a second confirmation step and must not infer funding from the requested dental status.

### Spoken confirmation before every write

- Before any AIDOO write, the assistant must say aloud exactly what it is about to do and show the same short summary in the assistant overlay.
- The assistant then enters an `awaitingConfirmation` state. It may execute the prepared action only after an unambiguous spoken confirmation such as `Да` or `Потвърждавам`.
- A negative or cancelling answer such as `Не`, `Откажи`, or `Отмени` discards the prepared action without issuing the write request.
- Silence, a new clinical instruction, or an ambiguous answer is not confirmation. The assistant asks again or cancels the draft; it must never treat ordinary conversation as permission to write.
- The spoken summary must contain the material clinical context needed to assess the action. For example: `Ще добавя оклузален кариес на зъб 32 в частен прием. Да го запиша ли?`
- Funding selection is part of the confirmed action. Creating a private visit therefore requires the assistant to say that it will create a private visit before it sends the observed `POST .../visits` request.

### Controlled surface-status attempt that did not persist

- A controlled UI attempt selected `Кариес (Оклузално / Инцизално / Куспидално)` for tooth `32` and used the editor's save flow.
- The subsequent status read-back in the UI still showed an empty current-status cell for tooth `32`. The editor remained active, so this attempt is not evidence of a successful surface-status write.
- No dedicated status-write request was retained in the Network log for this attempt. The exact interaction required by the current UI and the underlying surface-write contract both remain unknown.
- AIDOO Control must report success only after the independent teeth-status read contains the expected status and region. A navigation to the treatment view or a newly created visit is not sufficient verification.

### First controlled tooth-level status result

- A controlled UI test added the tooth-level label `Липсващ зъб` to tooth `23` in the designated test record.
- The change remained visible in the status history for the same visit after the editor closed. The history row therefore proves that the user-visible change persisted.
- The Network log had been cleared between the status editor action and the final visit action. The dedicated status-write request, if one was issued, is consequently not present in the retained capture. Its method, route, request body, response body, and failure behavior remain unobserved.
- This is persistence evidence, but it does not yet satisfy the write-contract acceptance criteria below. The test must be repeated with the Network log cleared immediately before the status editor's own save action.

### Visit finalization observed after the status change

- The final action in the same UI flow issued:
  `PUT https://aidoo-platform.on.dev-craft.tech/web/clinics/{clinicId}/patients/{patientId}/visits/{visitId}`
- Response status: `200` with `Content-Type: application/json`.
- Authentication header name: `X-Auth-Token`; its value is secret and is not recorded.
- Sanitized request body:

```text
{
  isFinished: true,
  note: null,
  nzokComplianceStatus: true
}
```

- The response was the updated visit object. Its observed fields match the visit-list shape and confirmed `createdStatusUpdate: true`, `isFinished: true`, and `nzokCompliancePassed: true`.
- This request finalizes the visit. It must not be treated as the contract for adding or replacing a dental status.
- The surrounding flow also called a local signing helper and attempted `POST /web/clinics/{clinicId}/ambulatory-sheets/send-signed`; the latter returned `400` in this observation. Those signing/compliance calls are separate from the still-unobserved dental-status write and require their own investigation.

## Acceptance for an observed write

A write is considered understood only when the same controlled change can be reproduced, the resulting state can be read back independently, the identifier mapping is stable across a reload, and failure/timeout behavior has been observed without automatically repeating the write.
