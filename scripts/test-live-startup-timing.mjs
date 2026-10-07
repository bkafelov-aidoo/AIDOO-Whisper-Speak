import assert from "node:assert/strict";
import test from "node:test";
import { LiveStartupTiming, describeLiveStartupMeasurement } from "../src/lib/live-startup-timing.ts";

test("separates microphone, ICE and session-request elapsed time without retaining values", async () => {
  let clock = 0;
  const records = [];
  const timing = new LiveStartupTiming((record) => records.push(record), () => clock);
  const privateValue = { token: "must-not-be-recorded", patient: "private-value" };
  for (const [stage, duration] of [
    ["microphone-acquisition", 7300],
    ["ice-gathering", 100],
    ["session-request", 900],
  ]) {
    assert.equal(await timing.measure(stage, async () => {
      clock += duration;
      return privateValue;
    }), privateValue);
  }
  timing.ready();
  timing.ready();
  assert.deepEqual(records, [
    { stage: "microphone-acquisition", outcome: "completed", elapsedMs: 7300, totalElapsedMs: 7300 },
    { stage: "ice-gathering", outcome: "completed", elapsedMs: 100, totalElapsedMs: 7400 },
    { stage: "session-request", outcome: "completed", elapsedMs: 900, totalElapsedMs: 8300 },
    { stage: "session-ready", outcome: "completed", elapsedMs: 8300, totalElapsedMs: 8300 },
  ]);
  assert.ok(!JSON.stringify(records).includes("must-not-be-recorded"));
  assert.ok(!JSON.stringify(records).includes("private-value"));
});

test("records a failed stage without storing its error or retrying it", async () => {
  let clock = 0;
  let attempts = 0;
  const records = [];
  const failure = new Error("private service error");
  const timing = new LiveStartupTiming((record) => records.push(record), () => clock);
  await assert.rejects(timing.measure("session-request", async () => {
    attempts += 1;
    clock = 1300;
    throw failure;
  }), (reason) => reason === failure);
  assert.equal(attempts, 1);
  assert.deepEqual(records, [
    { stage: "session-request", outcome: "failed", elapsedMs: 1300, totalElapsedMs: 1300 },
  ]);
  assert.ok(!JSON.stringify(records).includes(failure.message));
});

test("a broken diagnostic observer cannot fail startup or duplicate a ready event", async () => {
  let calls = 0;
  const timing = new LiveStartupTiming(() => {
    calls += 1;
    throw new Error("diagnostic unavailable");
  }, () => 0);
  assert.equal(await timing.measure("native-preparation", async () => "prepared"), "prepared");
  timing.ready();
  timing.ready();
  assert.equal(calls, 2);
});

test("without an observer startup adds no diagnostic work", async () => {
  const timing = new LiveStartupTiming(undefined, () => 0);
  assert.equal(await timing.measure("ice-gathering", async () => "complete"), "complete");
  timing.ready();
});

test("formats only bounded stage names, duration and outcome for the dev journal", () => {
  assert.equal(describeLiveStartupMeasurement({
    stage: "microphone-acquisition",
    outcome: "completed",
    elapsedMs: 1500,
    totalElapsedMs: 1600,
  }), "Старт — включване на микрофона: 1500 ms; общо 1600 ms; готово.");
});
