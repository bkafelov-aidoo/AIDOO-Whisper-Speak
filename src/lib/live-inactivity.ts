export const ASSISTANT_FEEDBACK_MS = 30_000;
export const ASSISTANT_INACTIVITY_MS = 60_000;
export const ASSISTANT_UNRECOGNIZED_SPEECH_MS = 4_000;
export const ASSISTANT_GOODBYE_START_TIMEOUT_MS = 5_000;
export const ASSISTANT_GOODBYE_SILENCE_MS = 900;

type Schedule = (callback: () => void, delayMs: number) => number;
type Cancel = (handle: number) => void;

export function inputSpeechState(eventType: string | undefined): "started" | "stopped" | null {
  if (!eventType) return null;
  if ([
    "input_audio_buffer.speech_started",
    "session.input_audio_buffer.speech_started",
    "session.input_audio.speech_started",
  ].includes(eventType)) return "started";
  if ([
    "input_audio_buffer.speech_stopped",
    "session.input_audio_buffer.speech_stopped",
    "session.input_audio.speech_stopped",
  ].includes(eventType)) return "stopped";
  return null;
}

export class LiveInactivityTimer {
  private handle: number | null = null;
  private active = false;
  private readonly onFeedback: () => void;
  private readonly onInactive: () => void;
  private readonly schedule: Schedule;
  private readonly cancel: Cancel;

  constructor(
    onFeedback: () => void,
    onInactive: () => void,
    schedule: Schedule = (callback, delayMs) => window.setTimeout(callback, delayMs),
    cancel: Cancel = (handle) => window.clearTimeout(handle),
  ) {
    this.onFeedback = onFeedback;
    this.onInactive = onInactive;
    this.schedule = schedule;
    this.cancel = cancel;
  }

  start() {
    this.active = true;
    this.arm();
  }

  touch() {
    if (this.active) this.arm();
  }

  feedbackNow() {
    if (!this.active) return;
    this.armClose();
    this.onFeedback();
  }

  pause() {
    this.active = false;
    this.clear();
  }

  stop() {
    this.active = false;
    this.clear();
  }

  private arm() {
    this.clear();
    this.handle = this.schedule(() => {
      this.handle = null;
      if (!this.active) return;
      this.armClose();
      this.onFeedback();
    }, ASSISTANT_FEEDBACK_MS);
  }

  private armClose() {
    this.clear();
    this.handle = this.schedule(() => {
      this.handle = null;
      if (!this.active) return;
      this.active = false;
      this.onInactive();
    }, ASSISTANT_INACTIVITY_MS - ASSISTANT_FEEDBACK_MS);
  }

  private clear() {
    if (this.handle !== null) this.cancel(this.handle);
    this.handle = null;
  }
}

export function assistantGreetingInstruction(eventId: string) {
  return {
    type: "session.instructions.append",
    event_id: eventId,
    delegation_id: null,
    content: "При стартиране на разговора поздрави веднага само със „Здравейте.“ на български, без въпрос или друг допълнителен текст.",
  } as const;
}

export function assistantGreetingPrompt(eventId: string) {
  return {
    type: "session.commentary.append",
    event_id: eventId,
    delegation_id: null,
    content: "Кажи сега само „Здравейте.“.",
  } as const;
}

export function assistantGoodbyeInstruction(eventId: string) {
  return {
    type: "session.instructions.append",
    event_id: eventId,
    delegation_id: null,
    content: "След 60 секунди без разбираема активност разговорът приключва. Кажи веднага само „Чао!“ на български, без допълнителен текст, след което замълчи.",
  } as const;
}

export function assistantClarificationInstruction(eventId: string) {
  return {
    type: "session.instructions.append",
    event_id: eventId,
    delegation_id: null,
    content: "Ако не е разпозната разбираема команда, кажи веднага само „Не ви чух добре. Моля, повторете.“ на български, без да приключваш разговора.",
  } as const;
}

export function assistantClarificationPrompt(eventId: string) {
  return {
    type: "session.commentary.append",
    event_id: eventId,
    delegation_id: null,
    content: "Кажи сега само „Не ви чух добре. Моля, повторете.“.",
  } as const;
}

export function assistantGoodbyePrompt(eventId: string) {
  return {
    type: "session.commentary.append",
    event_id: eventId,
    delegation_id: null,
    content: "Кажи сега само „Чао!“.",
  } as const;
}
