import assert from "node:assert/strict";
import test from "node:test";
import { consumePendingRequest, createEventScope } from "../src/lib/event-scope.ts";

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

function latchHarness(registration) {
  let listener;
  let pending = false;
  let starts = 0;
  let takes = 0;
  const scope = createEventScope(
    (_event, callback) => {
      listener = callback;
      return registration.promise;
    },
    (reason) => { throw reason; },
  );
  const drain = () => consumePendingRequest(
    async () => {
      takes += 1;
      const result = pending;
      pending = false;
      return result;
    },
    async () => { starts += 1; },
    () => true,
  );
  scope.listen("assistant:requested", () => { void drain(); }, () => { void drain(); });
  return {
    scope,
    listener: () => listener({ event: "assistant:requested", id: 1, payload: null }),
    request: () => { pending = true; },
    starts: () => starts,
    takes: () => takes,
  };
}

test("drains the native latch after listener registration repairs a lost wake event", async () => {
  const registration = deferred();
  const harness = latchHarness(registration);
  harness.request();
  registration.resolve(() => undefined);
  await settle();
  assert.equal(harness.starts(), 1);
  harness.scope.dispose();
});

test("a registration-edge event and ready drain consume one latch exactly once", async () => {
  const registration = deferred();
  const harness = latchHarness(registration);
  harness.request();
  harness.listener();
  registration.resolve(() => undefined);
  await settle();
  assert.equal(harness.starts(), 1);
  assert.equal(harness.takes(), 2);
  harness.scope.dispose();
});

test("registration and a notification without a pending wake never start the assistant", async () => {
  const registration = deferred();
  const harness = latchHarness(registration);
  registration.resolve(() => undefined);
  await settle();
  harness.listener();
  await settle();
  assert.equal(harness.takes(), 2);
  assert.equal(harness.starts(), 0);
  harness.scope.dispose();
});

test("disposing before registration completes removes the late listener without draining", async () => {
  const registration = deferred();
  const harness = latchHarness(registration);
  let unlistens = 0;
  harness.request();
  harness.scope.dispose();
  registration.resolve(() => { unlistens += 1; });
  await settle();
  harness.listener();
  await settle();
  assert.equal(unlistens, 1);
  assert.equal(harness.takes(), 0);
  assert.equal(harness.starts(), 0);
});

test("unmount during an in-flight latch read cannot start the assistant", async () => {
  const pending = deferred();
  let active = true;
  let starts = 0;
  const consuming = consumePendingRequest(
    () => pending.promise,
    async () => { starts += 1; },
    () => active,
  );
  active = false;
  pending.resolve(true);
  await consuming;
  assert.equal(starts, 0);
});
