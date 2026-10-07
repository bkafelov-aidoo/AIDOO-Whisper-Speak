import assert from "node:assert/strict";
import test from "node:test";
import {
  ASSISTANT_FEEDBACK_MS,
  ASSISTANT_INACTIVITY_MS,
  ASSISTANT_UNRECOGNIZED_SPEECH_MS,
  LiveInactivityTimer,
  assistantClarificationInstruction,
  assistantClarificationPrompt,
  assistantGreetingInstruction,
  assistantGreetingPrompt,
  assistantGoodbyeInstruction,
  assistantGoodbyePrompt,
  inputSpeechState,
} from "../src/lib/live-inactivity.ts";

function fakeClock() {
  let nextId = 0;
  const pending = new Map();
  return {
    schedule(callback, delayMs) {
      const id = ++nextId;
      pending.set(id, { callback, delayMs });
      return id;
    },
    cancel(id) { pending.delete(id); },
    fire() {
      const entry = [...pending.values()][0];
      pending.clear();
      entry?.callback();
    },
    pending() { return [...pending.values()]; },
  };
}

test("asks the user to repeat before ending after 60 seconds without activity", () => {
  const clock = fakeClock();
  let feedback = 0;
  let closes = 0;
  const timer = new LiveInactivityTimer(
    () => { feedback += 1; },
    () => { closes += 1; },
    clock.schedule,
    clock.cancel,
  );
  timer.start();
  assert.equal(ASSISTANT_FEEDBACK_MS, 30_000);
  assert.equal(ASSISTANT_INACTIVITY_MS, 60_000);
  assert.equal(clock.pending()[0].delayMs, ASSISTANT_FEEDBACK_MS);
  clock.fire();
  assert.equal(feedback, 1);
  assert.equal(closes, 0);
  assert.equal(clock.pending()[0].delayMs, ASSISTANT_INACTIVITY_MS - ASSISTANT_FEEDBACK_MS);
  clock.fire();
  assert.equal(closes, 1);
});

test("activity restarts the inactivity window and pause suppresses it", () => {
  const clock = fakeClock();
  let closes = 0;
  const timer = new LiveInactivityTimer(() => undefined, () => { closes += 1; }, clock.schedule, clock.cancel);
  timer.start();
  const first = clock.pending()[0];
  timer.touch();
  assert.notEqual(clock.pending()[0], first);
  timer.pause();
  timer.touch();
  clock.fire();
  assert.equal(closes, 0);
  timer.start();
  clock.fire();
  clock.fire();
  assert.equal(closes, 1);
});

test("recognized activity after the clarification restarts the full window", () => {
  const clock = fakeClock();
  let feedback = 0;
  let closes = 0;
  const timer = new LiveInactivityTimer(
    () => { feedback += 1; },
    () => { closes += 1; },
    clock.schedule,
    clock.cancel,
  );
  timer.start();
  clock.fire();
  assert.equal(feedback, 1);
  timer.touch();
  assert.equal(clock.pending()[0].delayMs, ASSISTANT_FEEDBACK_MS);
  clock.fire();
  assert.equal(feedback, 2);
  assert.equal(closes, 0);
});

test("unrecognized speech can request feedback immediately and keep 30 seconds to retry", () => {
  const clock = fakeClock();
  let feedback = 0;
  let closes = 0;
  const timer = new LiveInactivityTimer(
    () => { feedback += 1; },
    () => { closes += 1; },
    clock.schedule,
    clock.cancel,
  );
  timer.start();
  assert.equal(ASSISTANT_UNRECOGNIZED_SPEECH_MS, 4_000);
  timer.feedbackNow();
  assert.equal(feedback, 1);
  assert.equal(closes, 0);
  assert.equal(clock.pending()[0].delayMs, ASSISTANT_INACTIVITY_MS - ASSISTANT_FEEDBACK_MS);
});

test("recognizes both official Realtime and GPT-Live names for speech boundaries", () => {
  assert.equal(inputSpeechState("input_audio_buffer.speech_started"), "started");
  assert.equal(inputSpeechState("input_audio_buffer.speech_stopped"), "stopped");
  assert.equal(inputSpeechState("session.input_audio_buffer.speech_started"), "started");
  assert.equal(inputSpeechState("session.input_audio.speech_stopped"), "stopped");
  assert.equal(inputSpeechState("session.output_transcript.delta"), null);
});

test("a newly started live session asks for one exact greeting", () => {
  assert.deepEqual(assistantGreetingInstruction("hello_1"), {
    type: "session.instructions.append",
    event_id: "hello_1",
    delegation_id: null,
    content: "При стартиране на разговора поздрави веднага само със „Здравейте.“ на български, без въпрос или друг допълнителен текст.",
  });
  assert.deepEqual(assistantGreetingPrompt("hello_1_prompt"), {
    type: "session.commentary.append",
    event_id: "hello_1_prompt",
    delegation_id: null,
    content: "Кажи сега само „Здравейте.“.",
  });
});

test("idle close asks GPT-Live to say only goodbye before closing", () => {
  assert.deepEqual(assistantGoodbyeInstruction("idle_1"), {
    type: "session.instructions.append",
    event_id: "idle_1",
    delegation_id: null,
    content: "След 60 секунди без разбираема активност разговорът приключва. Кажи веднага само „Чао!“ на български, без допълнителен текст, след което замълчи.",
  });
  assert.deepEqual(assistantGoodbyePrompt("idle_1_prompt"), {
    type: "session.commentary.append",
    event_id: "idle_1_prompt",
    delegation_id: null,
    content: "Кажи сега само „Чао!“.",
  });
});

test("inactivity feedback asks for one clear repetition without closing", () => {
  assert.deepEqual(assistantClarificationInstruction("clarify_1"), {
    type: "session.instructions.append",
    event_id: "clarify_1",
    delegation_id: null,
    content: "Ако не е разпозната разбираема команда, кажи веднага само „Не ви чух добре. Моля, повторете.“ на български, без да приключваш разговора.",
  });
  assert.deepEqual(assistantClarificationPrompt("clarify_1_prompt"), {
    type: "session.commentary.append",
    event_id: "clarify_1_prompt",
    delegation_id: null,
    content: "Кажи сега само „Не ви чух добре. Моля, повторете.“.",
  });
});
