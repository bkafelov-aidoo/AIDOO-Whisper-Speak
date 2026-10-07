export const MAX_ASSISTANT_TRANSCRIPT_CHARS = 320;
export const ASSISTANT_CLOSE_GRACE_MS = 1_200;
export const ASSISTANT_BARE_END_DEBOUNCE_MS = 800;
export const ASSISTANT_INPUT_TURN_GAP_MS = 1_500;

const DICTATION_COMMANDS = [
  "започни транскрипция",
  "стартирай транскрипция",
  "започни да записваш",
  "стартирай запис",
  "запиши транскрипция",
  "start transcription",
  "start dictation",
] as const;

const END_SESSION_PATTERNS = [
  /(?:^| )край на (?:разговора|сесията)$/u,
  /(?:^| )затвори(?: ми)?$/u,
  /(?:^| )затвори (?:разговора|сесията|асистента|връзката)$/u,
  /(?:^| )приключи(?: разговора| сесията)?$/u,
  /(?:^| )приключваме(?: разговора| със сесията)?$/u,
  /(?:^| )приключихме(?: разговора| със сесията)?$/u,
  /(?:^| )прекрати (?:разговора|сесията)$/u,
  /(?:^| )спри (?:разговора|сесията|асистента|връзката)$/u,
  /(?:^| )довиждане$/u,
  /(?:^| )(?:end|goodbye)$/u,
  /(?:^| )(?:end|close|stop) (?:the )?(?:conversation|session|assistant)$/u,
] as const;

export type AssistantVoiceCommand = "start-dictation" | "end-session";

export function normalizeAssistantCommand(value: string) {
  return value
    .toLocaleLowerCase("bg-BG")
    .normalize("NFKC")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

export function matchesDictationCommand(value: string) {
  const normalized = normalizeAssistantCommand(value);
  return DICTATION_COMMANDS.some((command) => normalized.includes(command));
}

export function detectAssistantVoiceCommand(value: string): AssistantVoiceCommand | null {
  if (matchesDictationCommand(value)) return "start-dictation";
  if (quotesEndCommand(value)) return null;
  const normalized = normalizeAssistantCommand(value);
  if (reportedOrNegatedEndCommand(normalized)) return null;
  return matchesEndSessionText(normalized) ? "end-session" : null;
}

function matchesEndSessionText(value: string) {
  return value === "край" || END_SESSION_PATTERNS.some((pattern) => pattern.test(value));
}

function quotesEndCommand(value: string) {
  const quoted = /["'„“«]([^"'„“”«»]+)["'“”»]/gu;
  return [...value.matchAll(quoted)]
    .some((match) => matchesEndSessionText(normalizeAssistantCommand(match[1] ?? "")));
}

function reportedOrNegatedEndCommand(value: string) {
  return /(?:^| )(?:не|недей|без да|каза|казах|казва|кажи|повтори|произнеси|думата|фразата|примерът)(?: |$)/u
    .test(value);
}

/**
 * Collects fragmented GPT-Live transcript deltas and emits a command once.
 * Live transcript text stays in memory and is bounded so a long conversation
 * cannot grow the renderer's retained buffer indefinitely.
 */
export class AssistantCommandDetector {
  private transcript = "";
  private triggered = false;

  push(delta: string) {
    if (this.triggered || !delta) return false;
    this.transcript = `${this.transcript}${delta}`.slice(-MAX_ASSISTANT_TRANSCRIPT_CHARS);
    if (!matchesDictationCommand(this.transcript)) return false;
    this.triggered = true;
    return true;
  }

  reset() {
    this.transcript = "";
    this.triggered = false;
  }

  bufferedCharacterCount() {
    return this.transcript.length;
  }
}

export class AssistantVoiceCommandDetector {
  private transcript = "";
  private triggered = false;
  private pendingBareEnd: ReturnType<typeof globalThis.setTimeout> | null = null;
  private latestInputEndMs: number | null = null;
  private readonly options: AssistantVoiceCommandDetectorOptions;

  constructor(options: AssistantVoiceCommandDetectorOptions = {}) {
    this.options = options;
  }

  push(delta: string): AssistantVoiceCommand | null {
    if (this.triggered || !delta) return null;
    const hadPendingBareEnd = this.pendingBareEnd !== null;
    this.transcript = `${this.transcript}${delta}`.slice(-MAX_ASSISTANT_TRANSCRIPT_CHARS);
    if (hadPendingBareEnd) {
      if (normalizeAssistantCommand(delta) === "") return null;
      this.clearPendingBareEnd();
    }
    if (normalizeAssistantCommand(this.transcript) === "край") {
      this.pendingBareEnd = this.schedule(() => {
        this.pendingBareEnd = null;
        if (this.triggered) return;
        this.triggered = true;
        this.options.onDeferredCommand?.("end-session");
      }, ASSISTANT_BARE_END_DEBOUNCE_MS);
      return null;
    }
    const command = detectAssistantVoiceCommand(this.transcript);
    if (!command) return null;
    this.triggered = true;
    return command;
  }

  reset() {
    this.clearPendingBareEnd();
    this.transcript = "";
    this.triggered = false;
    this.latestInputEndMs = null;
  }

  beginInputTurn() {
    this.reset();
  }

  observeInputTiming(startMs: number | undefined, endMs: number | undefined) {
    if (typeof startMs !== "number" || !Number.isFinite(startMs) || startMs < 0
      || typeof endMs !== "number" || !Number.isFinite(endMs) || endMs < startMs) {
      return false;
    }
    const beginsNewTurn = this.latestInputEndMs !== null
      && startMs - this.latestInputEndMs >= ASSISTANT_INPUT_TURN_GAP_MS;
    if (beginsNewTurn) {
      this.clearPendingBareEnd();
      this.transcript = "";
      this.triggered = false;
      this.latestInputEndMs = endMs;
      return true;
    }
    this.latestInputEndMs = Math.max(this.latestInputEndMs ?? endMs, endMs);
    return false;
  }

  bufferedCharacterCount() {
    return this.transcript.length;
  }

  private schedule(callback: () => void, delayMs: number) {
    return this.options.setTimeout
      ? this.options.setTimeout(callback, delayMs)
      : globalThis.setTimeout(callback, delayMs);
  }

  private clearPendingBareEnd() {
    if (this.pendingBareEnd === null) return;
    if (this.options.clearTimeout) this.options.clearTimeout(this.pendingBareEnd);
    else globalThis.clearTimeout(this.pendingBareEnd);
    this.pendingBareEnd = null;
  }
}

export interface AssistantVoiceCommandDetectorOptions {
  onDeferredCommand?: (command: AssistantVoiceCommand) => void;
  setTimeout?: (
    callback: () => void,
    delayMs: number,
  ) => ReturnType<typeof globalThis.setTimeout>;
  clearTimeout?: (timer: ReturnType<typeof globalThis.setTimeout>) => void;
}

export interface LiveInputTranscriptEvent {
  type?: string;
  delta?: string;
}

export function detectAssistantVoiceCommandFromLiveEvent(
  event: LiveInputTranscriptEvent,
  detector: AssistantVoiceCommandDetector,
) {
  if (event.type !== "session.input_transcript.delta" || !event.delta) return null;
  return detector.push(event.delta);
}
