import assert from "node:assert/strict";
import test from "node:test";
import {
  OFFICIAL_NOTE_CONFIRMATION_QUESTION,
  OFFICIAL_NOTE_SILENCE_MS,
  OfficialNoteDictation,
} from "../src/lib/official-note-dictation.ts";

const noteCall = (callId, note) => ({
  type: "function_call",
  call_id: callId,
  name: "write_aidoo_official_note",
  arguments: JSON.stringify({
    patientId: "patient-1",
    tooth: "16",
    note,
    existingTreatmentId: null,
  }),
});

test("asks the exact fallback question after ten seconds without an ending phrase", () => {
  assert.equal(OFFICIAL_NOTE_SILENCE_MS, 10_000);
  const capture = new OfficialNoteDictation();
  assert.deepEqual(capture.intercept(noteCall("draft-1", "Пациентът е информиран.")), { kind: "wait" });
  const result = capture.finishSilence();
  assert.equal(result.callId, "draft-1");
  assert.deepEqual(JSON.parse(result.output), {
    ok: true,
    result: {
      saved: false,
      confirmationRequired: true,
      note: "Пациентът е информиран.",
      spokenSummary: "Да завършвам ли забележката?",
    },
  });
});

test("an ending phrase completes the note immediately and is not saved as note text", () => {
  const capture = new OfficialNoteDictation();
  const decision = capture.intercept(
    noteCall("write-1", "Пациентът е информиран. Готово, край на забележката."),
  );

  assert.equal(decision.kind, "execute");
  assert.equal(JSON.parse(decision.item.arguments).note, "Пациентът е информиран.");
});

test("a raw fragmented ending completes clean tool text without another wait", () => {
  const capture = new OfficialNoteDictation();
  capture.beginInputTurn();
  assert.equal(capture.pushTranscript("Пациентът е информиран. Край на забележ"), "none");
  assert.equal(capture.pushTranscript("ката."), "none");

  const decision = capture.intercept(noteCall("write-1", "Пациентът е информиран."));
  assert.equal(decision.kind, "execute");
  assert.equal(JSON.parse(decision.item.arguments).note, "Пациентът е информиран.");
});

test("an ending from an earlier input turn cannot complete a later note", () => {
  const capture = new OfficialNoteDictation();
  capture.beginInputTurn();
  capture.pushTranscript("Готово.");
  capture.beginInputTurn();
  capture.pushTranscript("Нова официална забележка.");

  assert.deepEqual(capture.intercept(noteCall("draft-1", "Нова официална забележка.")), { kind: "wait" });
});

test("an unrelated tool clears an observed ending before a later note", () => {
  const capture = new OfficialNoteDictation();
  capture.beginInputTurn();
  capture.pushTranscript("Готово.");
  assert.equal(capture.intercept({
    type: "function_call",
    call_id: "status-1",
    name: "read_aidoo_status",
    arguments: JSON.stringify({ patientId: "patient-1" }),
  }).kind, "unrelated");

  assert.deepEqual(capture.intercept(noteCall("draft-1", "Нова официална забележка.")), { kind: "wait" });
});

test("a fragmented ending phrase releases a held note without entering its text", () => {
  const capture = new OfficialNoteDictation();
  capture.intercept(noteCall("draft-1", "Пациентът е информиран."));
  assert.equal(capture.currentText(), "Пациентът е информиран.");
  assert.equal(capture.pushTranscript(" Контрол след шест месеца. Край на забележ"), "restart-silence");
  assert.equal(
    capture.currentText(),
    "Пациентът е информиран. Контрол след шест месеца. Край на забележ",
  );
  const decision = capture.pushTranscript("ката.");

  assert.equal(decision.kind, "finish");
  assert.equal(decision.callId, "draft-1");
  assert.deepEqual(JSON.parse(decision.output), {
    ok: true,
    result: {
      saved: false,
      confirmationRequired: false,
      readyToSave: true,
      note: "Пациентът е информиран. Контрол след шест месеца.",
      spokenSummary: "",
    },
  });
  assert.equal(capture.currentText(), "Пациентът е информиран. Контрол след шест месеца.");

  const save = capture.intercept({
    type: "function_call",
    call_id: "write-2",
    name: "write_aidoo_official_note",
    arguments: JSON.stringify({
      patientId: "patient-2",
      tooth: "26",
      note: "заместващ непълен текст",
      existingTreatmentId: "treatment-2",
    }),
  });
  assert.equal(save.kind, "execute");
  assert.deepEqual(JSON.parse(save.item.arguments), {
    patientId: "patient-1",
    tooth: "16",
    note: "Пациентът е информиран. Контрол след шест месеца.",
    existingTreatmentId: null,
  });
});

test("continued dictation restarts the ten second fallback and is preserved after confirmation", () => {
  const capture = new OfficialNoteDictation();
  capture.intercept(noteCall("draft-1", "Пациентът е информиран."));
  assert.equal(capture.pushTranscript(" Контрол след шест месеца."), "restart-silence");
  const result = capture.finishSilence();
  assert.equal(JSON.parse(result.output).result.note, "Пациентът е информиран. Контрол след шест месеца.");
  assert.equal(capture.pushTranscript("Да, завършена е."), "confirmed");

  const decision = capture.intercept(noteCall("write-2", "непълен текст"));
  assert.equal(decision.kind, "execute");
  assert.equal(
    JSON.parse(decision.item.arguments).note,
    "Пациентът е информиран. Контрол след шест месеца.",
  );
});

test("confirmed dictation keeps the original patient and treatment target", () => {
  const capture = new OfficialNoteDictation();
  capture.intercept(noteCall("draft-1", "Пациентът е информиран."));
  const target = capture.currentTarget();
  assert.deepEqual(target, {
    patientId: "patient-1",
    tooth: "16",
    existingTreatmentId: null,
  });
  assert.equal(Object.isFrozen(target), true);
  assert.throws(() => { target.patientId = "patient-2"; }, TypeError);
  assert.deepEqual(capture.currentTarget(), {
    patientId: "patient-1",
    tooth: "16",
    existingTreatmentId: null,
  });
  assert.equal(capture.pushTranscript(" Контрол след шест месеца."), "restart-silence");
  capture.finishSilence();
  assert.equal(capture.pushTranscript("Да, завършена е."), "confirmed");

  const decision = capture.intercept({
    type: "function_call",
    call_id: "write-2",
    name: "write_aidoo_official_note",
    arguments: JSON.stringify({
      patientId: "patient-2",
      tooth: "26",
      note: "заместващ непълен текст",
      existingTreatmentId: "treatment-2",
    }),
  });

  assert.equal(decision.kind, "execute");
  assert.deepEqual(JSON.parse(decision.item.arguments), {
    patientId: "patient-1",
    tooth: "16",
    note: "Пациентът е информиран. Контрол след шест месеца.",
    existingTreatmentId: null,
  });
  assert.equal(capture.currentTarget(), null);
});

test("current target rejects malformed patient and treatment identities", () => {
  const capture = new OfficialNoteDictation();
  assert.deepEqual(capture.intercept({
    ...noteCall("draft-1", "Бележка."),
    arguments: JSON.stringify({
      patientId: 42,
      tooth: "16",
      note: "Бележка.",
      existingTreatmentId: null,
    }),
  }), { kind: "wait" });
  assert.equal(capture.currentTarget(), null);
});

test("a second note call for another patient cannot replace an active capture", () => {
  const capture = new OfficialNoteDictation();
  assert.deepEqual(capture.intercept(noteCall("draft-1", "Оригинална забележка.")), { kind: "wait" });

  const duplicate = capture.intercept({
    type: "function_call",
    call_id: "duplicate-2",
    name: "write_aidoo_official_note",
    arguments: JSON.stringify({
      patientId: "patient-2",
      tooth: "26",
      note: "Чужд заместващ текст.",
      existingTreatmentId: "treatment-2",
    }),
  });

  assert.equal(duplicate.kind, "capture-in-progress");
  assert.equal(duplicate.callId, "duplicate-2");
  assert.deepEqual(JSON.parse(duplicate.output), {
    ok: true,
    result: {
      saved: false,
      captureInProgress: true,
      note: "Оригинална забележка.",
      spokenSummary: "",
    },
  });
  assert.deepEqual(capture.currentTarget(), {
    patientId: "patient-1",
    tooth: "16",
    existingTreatmentId: null,
  });
  const fallback = capture.finishSilence();
  assert.equal(fallback.callId, "draft-1");
  assert.equal(JSON.parse(fallback.output).result.note, "Оригинална забележка.");
});

test("a same-target retry cannot replace active text or its original timer call", () => {
  const capture = new OfficialNoteDictation();
  capture.intercept(noteCall("draft-1", "Оригинална забележка."));
  const duplicate = capture.intercept(noteCall("duplicate-2", "Непълен повторен текст."));

  assert.equal(duplicate.kind, "capture-in-progress");
  assert.equal(duplicate.callId, "duplicate-2");
  assert.equal(JSON.parse(duplicate.output).result.note, "Оригинална забележка.");
  assert.equal(capture.currentText(), "Оригинална забележка.");
  assert.deepEqual(capture.currentTarget(), {
    patientId: "patient-1",
    tooth: "16",
    existingTreatmentId: null,
  });
  assert.equal(capture.finishSilence().callId, "draft-1");
});

test("does not allow another write while confirmation is missing", () => {
  const capture = new OfficialNoteDictation();
  capture.intercept(noteCall("draft-1", "Бележка."));
  capture.finishSilence();
  const decision = capture.intercept(noteCall("early-write", "Бележка."));
  assert.equal(decision.kind, "ask-again");
  assert.equal(JSON.parse(decision.output).result.spokenSummary, OFFICIAL_NOTE_CONFIRMATION_QUESTION);
});
