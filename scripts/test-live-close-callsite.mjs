import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import {
  ASSISTANT_BARE_END_DEBOUNCE_MS,
  ASSISTANT_CLOSE_GRACE_MS,
  AssistantVoiceCommandDetector,
  detectAssistantVoiceCommandFromLiveEvent,
} from "../src/lib/assistant-command.ts";
import { OfficialNoteDictation } from "../src/lib/official-note-dictation.ts";

const source = await readFile(new URL("../src/hooks/useLiveConversation.ts", import.meta.url), "utf8");
const parsed = ts.createSourceFile("useLiveConversation.tsx", source, ts.ScriptTarget.Latest, true);
let messageCallback;
let stopCallback;
let requestVoiceCloseCallback;
function visit(node) {
  if (ts.isCallExpression(node) && ts.isPropertyAccessExpression(node.expression)
      && node.expression.name.text === "addEventListener"
      && ts.isStringLiteral(node.arguments[0]) && node.arguments[0].text === "message"
      && node.arguments[1]?.getText(parsed).includes("session.input_transcript.delta")) {
    messageCallback = node.arguments[1];
  }
  if (ts.isVariableDeclaration(node) && node.name.getText(parsed) === "stop"
      && node.initializer && ts.isCallExpression(node.initializer)) {
    stopCallback = node.initializer.arguments[0];
  }
  if (ts.isVariableDeclaration(node) && node.name.getText(parsed) === "requestVoiceClose"
      && node.initializer && ts.isCallExpression(node.initializer)) {
    requestVoiceCloseCallback = node.initializer.arguments[0];
  }
  ts.forEachChild(node, visit);
}
visit(parsed);
assert.ok(messageCallback, "Live transcript message callback is missing");
assert.ok(stopCallback, "Live stop callback is missing");
assert.ok(requestVoiceCloseCallback, "Voice-close callback is missing");

const compiledMessage = ts.transpileModule(
  `const callback = ${messageCallback.getText(parsed)};`,
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } },
).outputText;
const compiledStop = ts.transpileModule(
  `const callback = ${stopCallback.getText(parsed)};`,
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } },
).outputText;
const compiledRequestVoiceClose = ts.transpileModule(
  `const callback = ${requestVoiceCloseCallback.getText(parsed)};`,
  { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None } },
).outputText;

class VirtualTimers {
  nextId = 1;
  tasks = new Map();

  setTimeout(callback, delayMs) {
    const id = this.nextId++;
    this.tasks.set(id, { callback, delayMs });
    return id;
  }

  clearTimeout(id) {
    this.tasks.delete(id);
  }

  runAll() {
    const tasks = [...this.tasks.values()];
    this.tasks.clear();
    for (const { callback } of tasks) callback();
  }
}

function transcriptHarness(noteDecision = "none", noteText = null, actualOfficialNote = null) {
  const timers = new VirtualTimers();
  let stopCalls = 0;
  let notePushes = 0;
  let noteInputTurns = 0;
  const diagnostics = [];
  const toasts = [];
  const toolOutputs = [];
  const stopRef = { current: () => { stopCalls += 1; } };
  const officialNoteRef = actualOfficialNote ? { current: actualOfficialNote } : {
    current: {
      pushTranscript: (delta) => {
        notePushes += 1;
        return typeof noteDecision === "function" ? noteDecision(delta) : noteDecision;
      },
      beginInputTurn: () => { noteInputTurns += 1; },
      currentText: () => noteText,
    },
  };
  const requestVoiceClose = new Function(
    "scope",
    `with (scope) { ${compiledRequestVoiceClose}; return callback; }`,
  )({
    officialNoteRef,
    addDiagnostic: (kind, detail) => diagnostics.push({ kind, detail }),
    emitTo: (target, event, payload) => {
      toasts.push({ target, event, payload });
      return Promise.resolve();
    },
    stopRef,
  });
  const voiceCloseRef = { current: requestVoiceClose };
  const detector = new AssistantVoiceCommandDetector({
    onDeferredCommand: (command) => {
      if (command === "end-session") voiceCloseRef.current();
    },
    setTimeout: (callback, delayMs) => timers.setTimeout(callback, delayMs),
    clearTimeout: (id) => timers.clearTimeout(id),
  });
  const scope = {
    inputSpeechState: () => null,
    closingRef: { current: false },
    speechRecognizedRef: { current: false },
    addTranscriptDelta() {},
    registerUserActivity() {},
    officialNoteRef,
    clearOfficialNoteTimer() {},
    publishOfficialNotePreview() {},
    addDiagnostic() {},
    sendAidooToolOutput: (_channel, callId, output) => toolOutputs.push({ callId, output }),
    updatePhase() {},
    inactivityTimerRef: { current: { start() {} } },
    armOfficialNoteTimer() {},
    commandDetectorRef: { current: detector },
    detectAssistantVoiceCommandFromLiveEvent,
    switchToDictation() {},
    stopRef,
    voiceCloseRef,
    clearTimer() {},
    readyRef: { current: false },
    greetingInstructionIdRef: { current: null },
    goodbyeInstructionIdRef: { current: null },
    channel: { send() {} },
  };
  const callback = new Function("scope", `with (scope) { ${compiledMessage}; return callback; }`)(scope);
  return {
    detector,
    pendingCount: () => timers.tasks.size,
    pendingDelays: () => [...timers.tasks.values()].map(({ delayMs }) => delayMs),
    runAll: () => timers.runAll(),
    send(delta, timing = {}) {
      callback({
        data: JSON.stringify({ type: "session.input_transcript.delta", delta, ...timing }),
      });
    },
    stopCalls: () => stopCalls,
    notePushes: () => notePushes,
    noteInputTurns: () => noteInputTurns,
    diagnostics: () => diagnostics,
    toasts: () => toasts,
    toolOutputs: () => toolOutputs,
  };
}

function closingMessageHarness() {
  let toolExecutions = 0;
  let finishes = 0;
  let failures = 0;
  const closingRef = { current: true };
  const scope = {
    inputSpeechState: () => null,
    closingRef,
    goodbyeInstructionIdRef: { current: null },
    clarificationActiveRef: { current: false },
    inactivityTimerRef: { current: { touch() {}, pause() {}, start() {} } },
    addTranscriptDelta() {},
    addDiagnostic() {},
    describeLiveToolCall: () => "late tool",
    describeLiveToolResult: () => "late result",
    backendUsageFromLiveEvent: () => null,
    invoke: () => Promise.resolve(),
    functionCallFromLiveEvent: () => ({
      call_id: "late-call",
      name: "write_aidoo_status",
      arguments: "{}",
    }),
    handledToolCallsRef: { current: new Set() },
    officialNoteRef: {
      current: {
        intercept: (item) => ({ kind: "unrelated", item }),
      },
    },
    clearOfficialNoteTimer() {},
    publishOfficialNotePreview() {},
    updatePhase() {},
    armOfficialNoteTimer() {},
    sendAidooToolOutput() {},
    toolBusyRef: { current: false },
    executeAidooLiveTool: () => {
      toolExecutions += 1;
      return Promise.resolve({ callId: "late-call", output: "{}" });
    },
    channel: { readyState: "open", send() {} },
    finish: () => { finishes += 1; },
    finishAfterLocalGoodbye() {},
    fail: () => { failures += 1; },
  };
  const callback = new Function("scope", `with (scope) { ${compiledMessage}; return callback; }`)(scope);
  return {
    send(event) {
      callback({ data: JSON.stringify(event) });
    },
    toolExecutions: () => toolExecutions,
    finishes: () => finishes,
    failures: () => failures,
  };
}

test("bare край closes once after a bounded transcript-fragment guard", () => {
  const run = transcriptHarness();
  run.send("Край");
  assert.equal(run.stopCalls(), 0);
  assert.equal(run.pendingCount(), 1);
  assert.equal(run.pendingDelays()[0], ASSISTANT_BARE_END_DEBOUNCE_MS);
  run.runAll();
  run.runAll();
  assert.equal(run.stopCalls(), 1);
});

test("a timestamped standalone край closes after an earlier clinical utterance", () => {
  const run = transcriptHarness();
  run.send("Покажи статуса на зъб едно шест", { start_ms: 1_000, end_ms: 2_000 });
  run.send("Край", { start_ms: 4_000, end_ms: 4_400 });
  assert.equal(run.noteInputTurns(), 1);
  assert.equal(run.pendingCount(), 1);
  run.runAll();
  assert.equal(run.stopCalls(), 1);
});

test("short and overlapping timestamp fragments remain one utterance", () => {
  for (const timing of [
    { start_ms: 2_400, end_ms: 2_700 },
    { start_ms: 900, end_ms: 1_200 },
    {},
    { start_ms: 3_000, end_ms: 2_000 },
  ]) {
    const run = transcriptHarness();
    run.send("Кра", { start_ms: 1_000, end_ms: 1_000 });
    run.send("й", timing);
    assert.equal(run.pendingCount(), 1, JSON.stringify(timing));
  }
});

test("unambiguous natural disconnect commands close immediately once", () => {
  for (const phrase of ["Затвори връзката", "Спри връзката", "Приключихме"]) {
    const run = transcriptHarness();
    run.send(phrase);
    assert.equal(run.stopCalls(), 1, phrase);
    assert.equal(run.pendingCount(), 0, phrase);
  }
});

test("fragmented край на забележката cancels the queued session close", () => {
  const run = transcriptHarness();
  run.send("Край");
  assert.equal(run.pendingCount(), 1);
  run.send(" на забележката");
  assert.equal(run.pendingCount(), 0);
  run.runAll();
  assert.equal(run.stopCalls(), 0);
});

test("fragmented explicit session ending is re-evaluated after bare край", () => {
  const run = transcriptHarness();
  run.send("Край");
  run.send(" на разговора");
  assert.equal(run.pendingCount(), 0);
  assert.equal(run.stopCalls(), 1);
});

test("punctuation-only deltas keep the bare край guard armed", () => {
  for (const continuation of [".", " "]) {
    const run = transcriptHarness();
    run.send("Край");
    run.send(continuation);
    assert.equal(run.pendingCount(), 1, continuation);
    run.runAll();
    assert.equal(run.stopCalls(), 1, continuation);
  }
});

test("meaningful continuation cancels a queued bare край", () => {
  const run = transcriptHarness();
  run.send("Край");
  run.send(" благодаря");
  run.runAll();
  assert.equal(run.stopCalls(), 0);
  assert.equal(run.pendingCount(), 0);
});

test("negated and clinical uses of край never queue a close", () => {
  for (const phrase of [
    "Не казвай край.",
    "Опиши дисталния край на короната.",
    "Край на забележката.",
  ]) {
    const run = transcriptHarness();
    run.send(phrase);
    run.runAll();
    assert.equal(run.stopCalls(), 0, phrase);
    assert.equal(run.pendingCount(), 0, phrase);
  }
});

test("reset clears a queued bare край before session cleanup", () => {
  const run = transcriptHarness();
  assert.equal(run.detector.observeInputTiming(1_000, 2_000), false);
  run.send("Край");
  assert.equal(run.pendingCount(), 1);
  run.detector.reset();
  assert.equal(run.pendingCount(), 0);
  assert.equal(run.detector.observeInputTiming(4_000, 4_400), false);
  run.runAll();
  assert.equal(run.stopCalls(), 0);
});

test("standalone close commands are not swallowed by pending official-note capture", () => {
  const stops = {};
  for (const phrase of ["Край", "Затвори", "Приключихме"]) {
    const run = transcriptHarness("restart-silence", "Незавършена забележка");
    run.send(phrase);
    if (phrase === "Край") run.runAll();
    stops[phrase] = run.stopCalls();
    assert.equal(run.notePushes(), phrase === "Край" ? 1 : 0, phrase);
    assert.deepEqual(run.diagnostics(), [
      { kind: "system", detail: "Забележката не е записана." },
    ], phrase);
    assert.deepEqual(run.toasts(), [{
      target: "overlay",
      event: "toast",
      payload: "Забележката не е записана.",
    }], phrase);
  }
  assert.deepEqual(stops, { Край: 1, Затвори: 1, Приключихме: 1 });
});

test("pending official-note capture preserves fragmented край на забележката", () => {
  let fragment = 0;
  const run = transcriptHarness(() => {
    fragment += 1;
    return fragment === 1
      ? "restart-silence"
      : { kind: "finish", callId: "note-call", output: "{}" };
  });
  run.send("Край");
  run.send(" на забележката");
  run.runAll();
  assert.equal(run.stopCalls(), 0);
  assert.equal(run.pendingCount(), 0);
});

test("actual official-note capture distinguishes note ending from session ending", () => {
  const argumentsToSave = {
    patientId: "patient-original",
    tooth: "18",
    existingTreatmentId: "treatment-original",
    note: "Оригинален текст",
  };
  const beginCapture = () => {
    const note = new OfficialNoteDictation();
    assert.deepEqual(note.intercept({
      call_id: "original-call",
      name: "write_aidoo_official_note",
      arguments: JSON.stringify(argumentsToSave),
    }), { kind: "wait" });
    return note;
  };

  const noteEnding = beginCapture();
  const finishRun = transcriptHarness("none", null, noteEnding);
  finishRun.send("Край", { start_ms: 1_000, end_ms: 1_300 });
  finishRun.send(" на забележката", { start_ms: 1_300, end_ms: 2_000 });
  finishRun.runAll();
  assert.equal(finishRun.stopCalls(), 0);
  assert.equal(finishRun.toolOutputs().length, 1);
  const finishOutput = JSON.parse(finishRun.toolOutputs()[0].output);
  assert.equal(finishOutput.result.readyToSave, true);
  assert.equal(finishOutput.result.note, "Оригинален текст");
  assert.deepEqual(noteEnding.currentTarget(), {
    patientId: "patient-original",
    tooth: "18",
    existingTreatmentId: "treatment-original",
  });

  const sessionEnding = beginCapture();
  const closeRun = transcriptHarness("none", null, sessionEnding);
  closeRun.send("Край", { start_ms: 1_000, end_ms: 1_300 });
  closeRun.runAll();
  assert.equal(closeRun.stopCalls(), 1);
  assert.equal(closeRun.toolOutputs().length, 0);
  assert.deepEqual(closeRun.diagnostics(), [
    { kind: "system", detail: "Забележката не е записана." },
  ]);
  assert.deepEqual(sessionEnding.currentTarget(), {
    patientId: "patient-original",
    tooth: "18",
    existingTreatmentId: "treatment-original",
  });
});

test("the actual stop callback releases the microphone and closes only once", () => {
  const timers = new VirtualTimers();
  let trackStops = 0;
  let closeEvents = 0;
  let finishes = 0;
  const scope = {
    closingRef: { current: false },
    operationRef: { current: 0 },
    updatePhase() {},
    microphoneRef: { current: { getTracks: () => [{ stop: () => { trackStops += 1; } }] } },
    channelRef: {
      current: {
        readyState: "open",
        send: () => { closeEvents += 1; },
      },
    },
    readyRef: { current: true },
    clearTimer() {},
    timeoutRef: { current: null },
    finish: () => { finishes += 1; },
    window: { setTimeout: (callback, delayMs) => timers.setTimeout(callback, delayMs) },
    ASSISTANT_CLOSE_GRACE_MS,
  };
  const stop = new Function("scope", `with (scope) { ${compiledStop}; return callback; }`)(scope);
  stop();
  stop();
  assert.equal(trackStops, 1);
  assert.equal(closeEvents, 1);
  assert.equal(timers.tasks.size, 1);
  timers.runAll();
  assert.equal(finishes, 1);
});

test("closing ignores a late clinical function call but still handles terminal events", async () => {
  const run = closingMessageHarness();
  run.send({ type: "response.event", event: { type: "response.function_call_arguments.done" } });
  await Promise.resolve();
  assert.equal(run.toolExecutions(), 0);

  run.send({ type: "session.closed" });
  run.send({ type: "error", error: { message: "closed" } });
  assert.equal(run.finishes(), 1);
  assert.equal(run.failures(), 1);
});
