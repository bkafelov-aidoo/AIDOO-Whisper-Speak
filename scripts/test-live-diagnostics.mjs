import assert from "node:assert/strict";
import test from "node:test";
import {
  MAX_LIVE_DIAGNOSTIC_DETAIL_CHARS,
  MAX_LIVE_DIAGNOSTIC_ENTRIES,
  appendLiveDiagnostic,
  appendLiveTranscriptDelta,
  describeLiveToolCall,
  describeLiveToolResult,
} from "../src/lib/live-diagnostics.ts";

test("keeps fragmented patient names together in the visible transcript", () => {
  let entries = [];
  entries = appendLiveTranscriptDelta(entries, "heard", "Намери пациента Моника ", 1);
  entries = appendLiveTranscriptDelta(entries, "heard", "Станева", 2);

  assert.equal(entries.length, 1);
  assert.equal(entries[0].detail, "Намери пациента Моника Станева");
  assert.equal(entries[0].at, 2);
});

test("starts a new transcript row after another diagnostic event", () => {
  let entries = appendLiveTranscriptDelta([], "heard", "Първа команда", 1);
  entries = appendLiveDiagnostic(entries, { kind: "tool-call", detail: "search_aidoo_patients", at: 2 });
  entries = appendLiveTranscriptDelta(entries, "heard", "Втора команда", 3);

  assert.deepEqual(entries.map((entry) => entry.kind), ["heard", "tool-call", "heard"]);
});

test("shows parsed tool arguments and returned success or error", () => {
  assert.equal(
    describeLiveToolCall("search_aidoo_patients", '{"query":"Моника Станева"}'),
    'search_aidoo_patients\n{\n  "query": "Моника Станева"\n}',
  );
  assert.equal(
    describeLiveToolResult("search_aidoo_patients", '{"ok":false,"error":"Пациентът не е намерен"}'),
    'search_aidoo_patients\n{\n  "ok": false,\n  "error": "Пациентът не е намерен"\n}',
  );
});

test("bounds retained entries and long payloads", () => {
  let entries = [];
  for (let index = 0; index < MAX_LIVE_DIAGNOSTIC_ENTRIES + 12; index += 1) {
    entries = appendLiveDiagnostic(entries, { kind: "system", detail: `event-${index}`, at: index });
  }
  entries = appendLiveDiagnostic(entries, {
    kind: "error",
    detail: "x".repeat(MAX_LIVE_DIAGNOSTIC_DETAIL_CHARS * 2),
    at: 999,
  });

  assert.equal(entries.length, MAX_LIVE_DIAGNOSTIC_ENTRIES);
  assert.ok(entries.at(-1).detail.length <= MAX_LIVE_DIAGNOSTIC_DETAIL_CHARS);
  assert.equal(entries[0].detail, "event-13");
});
