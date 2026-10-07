import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import { LiveStartupTiming, describeLiveStartupMeasurement } from "../src/lib/live-startup-timing.ts";
import { microphoneAcquireOptionsForDevice } from "../src/lib/live-microphone.ts";

// Execute the actual startup callback with virtual media/network boundaries, not a copied flow.
const source = await readFile(new URL("../src/hooks/useLiveConversation.ts", import.meta.url), "utf8");
const parsed = ts.createSourceFile("startup.tsx", source, ts.ScriptTarget.Latest, true);
let startupCallback;
function visit(node) {
  if (ts.isVariableDeclaration(node) && node.name.getText(parsed) === "start"
      && node.initializer && ts.isCallExpression(node.initializer)) {
    startupCallback = node.initializer.arguments[0];
  }
  ts.forEachChild(node, visit);
}
visit(parsed);
assert.ok(startupCallback, "live startup callback is missing");
const compiled = ts.transpileModule(`const startup = ${startupCallback.getText(parsed)};`, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
}).outputText;

function harness() {
  let clock = 0;
  const diagnostics = [];
  const calls = [];
  let microphoneOptions;
  const track = { label: "test microphone", readyState: "live", getSettings: () => ({}), stop() {} };
  const microphone = { getTracks: () => [track], getAudioTracks: () => [track] };
  const refs = Object.fromEntries([
    "closingRef", "clarificationActiveRef", "switchingRef", "readyRef", "mountedRef",
    "peerRef", "channelRef", "microphoneRef", "timeoutRef", "greetingInstructionIdRef",
    "previousSuccessfulMicrophoneRef",
  ].map((name) => [name, { current: name === "mountedRef" ? true : null }]));
  refs.previousSuccessfulMicrophoneRef.current = undefined;
  class FakePeer {
    localDescription = { sdp: "fake offer" };
    addTrack() {}
    addEventListener() {}
    createDataChannel() { return { addEventListener() {}, send() {} }; }
    async createOffer() { clock += 20; return { sdp: "fake offer" }; }
    async setLocalDescription() { clock += 10; }
    async setRemoteDescription() { clock += 10; }
  }
  const scope = {
    ...refs,
    microphoneDeviceRevisionRef: { current: 0 },
    operationRef: { current: 0 },
    commandDetectorRef: { current: { reset() {} } },
    phase: "idle",
    microphoneName: null,
    diagnosticsEnabled: true,
    setError() {},
    updatePhase() {},
    addDiagnostic: (_kind, detail) => diagnostics.push(detail),
    invoke: async (command) => {
      calls.push(command);
      clock += command === "prepare_live_session" ? 30 : 800;
      return command === "create_live_session" ? { sdp: "fake answer" } : undefined;
    },
    delay: async (milliseconds) => { clock += milliseconds; },
    RTCPeerConnection: FakePeer,
    navigator: { mediaDevices: { enumerateDevices: async () => [], getUserMedia: async () => microphone } },
    acquireMicrophone: async (_request, _constraints, options) => {
      microphoneOptions = options;
      calls.push("getUserMedia");
      clock += 15;
      return microphone;
    },
    LiveStartupTiming: class extends LiveStartupTiming {
      constructor(observer) { super(observer, () => clock); }
    },
    describeLiveStartupMeasurement,
    microphoneAcquireOptionsForDevice,
    waitForIceGathering: async () => { clock += 100; },
    withTimeout: async (promise) => promise,
    window: { setTimeout: () => 1 },
    MICROPHONE_ATTEMPT_TIMEOUT_MS: 7000,
    MICROPHONE_RETRY_DELAY_MS: 300,
    LIVE_CREATE_TIMEOUT_MS: 50000,
    SESSION_START_TIMEOUT_MS: 20000,
    fail: (error) => { throw error; },
  };
  // The production hook's lexical dependencies are supplied without running React/native effects.
  const start = new Function("scope", `with (scope) { ${compiled}; return startup; }`)(scope);
  return { start, diagnostics, calls, get microphoneOptions() { return microphoneOptions; }, scope };
}

test("the real startup callback measures microphone, ICE and service requests separately", async () => {
  const run = harness();
  await run.start();
  assert.deepEqual(run.calls, ["prepare_live_session", "getUserMedia", "create_live_session"]);
  for (const label of ["подготовка на приложението", "включване на микрофона", "подготовка на мрежовата връзка", "заявка за гласова сесия", "прилагане на отговора"]) {
    assert.ok(run.diagnostics.some((entry) => entry.startsWith(`Старт — ${label}:`)), label);
  }
});

test("startup phase timings remain hidden when dev diagnostics are disabled", async () => {
  const run = harness();
  run.scope.diagnosticsEnabled = false;
  await run.start();
  assert.ok(!run.diagnostics.some((entry) => entry.startsWith("Старт —")));
});

test("the actual callsite shortens only a previously acquired same microphone", async () => {
  const run = harness();
  await run.start();
  assert.equal(run.microphoneOptions.firstAttemptTimeoutMs, 7000);
  assert.equal(run.scope.previousSuccessfulMicrophoneRef.current, null);
  await run.start();
  assert.equal(run.microphoneOptions.firstAttemptTimeoutMs, 1500);
  assert.equal(run.microphoneOptions.attemptTimeoutMs, 7000);
  assert.equal(run.microphoneOptions.retryDelayMs, 100);
  run.scope.microphoneName = "unavailable selected microphone";
  run.scope.previousSuccessfulMicrophoneRef.current = run.scope.microphoneName;
  await run.start();
  assert.equal(run.microphoneOptions.firstAttemptTimeoutMs, 7000);
  assert.equal(run.scope.previousSuccessfulMicrophoneRef.current, undefined);
});

test("a canceled acquisition cannot cache warm proof, open the session or append late timings", async () => {
  const run = harness();
  const acquire = run.scope.acquireMicrophone;
  run.scope.acquireMicrophone = async (...arguments_) => {
    const stream = await acquire(...arguments_);
    run.scope.operationRef.current += 1;
    assert.equal(arguments_[2].shouldRetry(), false);
    return stream;
  };
  await run.start();
  assert.equal(run.scope.previousSuccessfulMicrophoneRef.current, undefined);
  assert.deepEqual(run.calls, ["prepare_live_session", "getUserMedia"]);
  assert.ok(!run.diagnostics.some((entry) => entry.startsWith("Старт — включване на микрофона")));
});

test("an empty or ended audio stream is never accepted as warm microphone proof", async () => {
  for (const tracks of [[], [{ readyState: "ended", getSettings: () => ({}), stop() {} }]]) {
    const run = harness();
    run.scope.acquireMicrophone = async () => ({ getTracks: () => tracks, getAudioTracks: () => tracks });
    await run.start();
    assert.equal(run.scope.previousSuccessfulMicrophoneRef.current, undefined);
  }
});

test("changing media devices or the selected microphone invalidates the warm proof", () => {
  const effects = [];
  function collect(node) {
    if (ts.isCallExpression(node) && node.expression.getText(parsed) === "useEffect"
        && node.arguments[0]?.getText(parsed).includes("previousSuccessfulMicrophoneRef.current = undefined")) {
      effects.push(node.arguments[0]);
    }
    ts.forEachChild(node, collect);
  }
  collect(parsed);
  assert.equal(effects.length, 2);
  for (const effect of effects) {
    const run = harness();
    run.scope.previousSuccessfulMicrophoneRef.current = null;
    let deviceChange;
    run.scope.navigator.mediaDevices.addEventListener = (_type, listener) => { deviceChange = listener; };
    run.scope.navigator.mediaDevices.removeEventListener = (_type, listener) => { assert.equal(listener, deviceChange); };
    const compiledEffect = ts.transpileModule(`const effect = ${effect.getText(parsed)};`, {
      compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
    }).outputText;
    const cleanup = new Function("scope", `with (scope) { ${compiledEffect}; return effect(); }`)(run.scope);
    deviceChange?.();
    assert.equal(run.scope.previousSuccessfulMicrophoneRef.current, undefined);
    assert.equal(run.scope.microphoneDeviceRevisionRef.current, 1);
    cleanup?.();
  }
});
