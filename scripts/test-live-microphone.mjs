import assert from "node:assert/strict";
import test from "node:test";
import {
  acquireMicrophone,
  mediaRequestWithTimeout,
  microphoneAcquireOptionsForDevice,
} from "../src/lib/live-microphone.ts";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function fakeStream() {
  const track = { stopped: false, stop() { this.stopped = true; } };
  return { stream: { getTracks: () => [track] }, track };
}

test("stops a media stream that arrives after its request timed out", async () => {
  const pending = deferred();
  const late = fakeStream();
  await assert.rejects(
    mediaRequestWithTimeout(pending.promise, 5, "timeout"),
    /timeout/,
  );
  pending.resolve(late.stream);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(late.track.stopped, true);
});

test("retries one timed-out microphone handoff and returns the fresh stream", async () => {
  const first = deferred();
  const second = fakeStream();
  let attempts = 0;
  const acquired = acquireMicrophone(
    () => {
      attempts += 1;
      return attempts === 1 ? first.promise : Promise.resolve(second.stream);
    },
    { audio: true },
    { attemptTimeoutMs: 5, retryDelayMs: 0 },
  );
  assert.equal(await acquired, second.stream);
  assert.equal(attempts, 2);

  const late = fakeStream();
  first.resolve(late.stream);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(late.track.stopped, true);
});

test("does not retry a permission rejection", async () => {
  let attempts = 0;
  await assert.rejects(
    acquireMicrophone(
      () => {
        attempts += 1;
        return Promise.reject(Object.assign(new Error("denied"), { name: "NotAllowedError" }));
      },
      { audio: true },
      { attemptTimeoutMs: 5, retryDelayMs: 0 },
    ),
    /denied/,
  );
  assert.equal(attempts, 1);
});

test("keeps the cold microphone budget and shortens only a proven warm handoff", () => {
  const cold = microphoneAcquireOptionsForDevice("Desk microphone", undefined);
  assert.deepEqual(cold, {
    attemptTimeoutMs: 7_000,
    firstAttemptTimeoutMs: 7_000,
    retryDelayMs: 300,
  });
  assert.equal(cold.firstAttemptTimeoutMs + cold.retryDelayMs, 7_300);

  const warm = microphoneAcquireOptionsForDevice("Desk microphone", "Desk microphone");
  assert.deepEqual(warm, {
    attemptTimeoutMs: 7_000,
    firstAttemptTimeoutMs: 1_500,
    retryDelayMs: 100,
  });
  assert.equal(warm.firstAttemptTimeoutMs + warm.retryDelayMs, 1_600);

  const changed = microphoneAcquireOptionsForDevice("Room microphone", "Desk microphone");
  assert.deepEqual(changed, cold);
  const warmSystemDefault = microphoneAcquireOptionsForDevice(null, null);
  assert.deepEqual(warmSystemDefault, warm);
});

test("uses the short first budget but keeps the full final attempt", async () => {
  const first = deferred();
  const second = deferred();
  const events = [];
  let attempts = 0;
  let retryChecks = 0;
  const acquired = acquireMicrophone(
    () => {
      attempts += 1;
      return attempts === 1 ? first.promise : second.promise;
    },
    { audio: true },
    {
      attemptTimeoutMs: 60,
      firstAttemptTimeoutMs: 5,
      retryDelayMs: 1,
      attemptObserver: (event) => events.push(event),
      shouldRetry: () => {
        retryChecks += 1;
        return true;
      },
    },
  );

  await new Promise((resolve) => setTimeout(resolve, 15));
  assert.equal(attempts, 2);
  const fresh = fakeStream();
  second.resolve(fresh.stream);
  assert.equal(await acquired, fresh.stream);
  assert.equal(retryChecks, 2);
  assert.deepEqual(events.map(({ attempt, state }) => ({ attempt, state })), [
    { attempt: 1, state: "started" },
    { attempt: 1, state: "timed-out" },
    { attempt: 2, state: "started" },
    { attempt: 2, state: "succeeded" },
  ]);
  assert.ok(events.every((event) => (
    event.elapsedMs >= 0
      && Object.keys(event).sort().join(",") === "attempt,elapsedMs,state"
  )));

  const late = fakeStream();
  first.resolve(late.stream);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(late.track.stopped, true);
});

test("observer failures cannot control microphone acquisition", async () => {
  const stream = fakeStream();
  const acquired = await acquireMicrophone(
    () => Promise.resolve(stream.stream),
    { audio: true },
    {
      attemptTimeoutMs: 20,
      retryDelayMs: 0,
      attemptObserver: () => { throw new Error("observer failure"); },
    },
  );
  assert.equal(acquired, stream.stream);
});

test("reports a permission rejection without retrying or exposing the error", async () => {
  const events = [];
  let attempts = 0;
  await assert.rejects(
    acquireMicrophone(
      () => {
        attempts += 1;
        return Promise.reject(Object.assign(new Error("private device detail"), {
          name: "NotAllowedError",
        }));
      },
      { audio: true },
      {
        attemptTimeoutMs: 20,
        retryDelayMs: 0,
        attemptObserver: (event) => events.push(event),
      },
    ),
    /private device detail/,
  );
  assert.equal(attempts, 1);
  assert.deepEqual(events.map(({ attempt, state }) => ({ attempt, state })), [
    { attempt: 1, state: "started" },
    { attempt: 1, state: "rejected" },
  ]);
  assert.ok(events.every((event) => JSON.stringify(event).includes("private device detail") === false));
});

test("a stopped session cannot launch the retry attempt", async () => {
  const first = deferred();
  let attempts = 0;
  let retryChecks = 0;
  let active = true;
  const stopDuringDelay = setTimeout(() => { active = false; }, 10);
  await assert.rejects(
    acquireMicrophone(
      () => {
        attempts += 1;
        return first.promise;
      },
      { audio: true },
      {
        attemptTimeoutMs: 30,
        firstAttemptTimeoutMs: 5,
        retryDelayMs: 20,
        shouldRetry: () => {
          retryChecks += 1;
          return active;
        },
      },
    ),
    /навреме/,
  );
  clearTimeout(stopDuringDelay);
  assert.equal(retryChecks, 2);
  assert.equal(attempts, 1);

  const late = fakeStream();
  first.resolve(late.stream);
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.equal(late.track.stopped, true);
});
