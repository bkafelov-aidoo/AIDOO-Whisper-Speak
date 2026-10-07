import type { FunctionCallItem } from "./aidoo-live-tools";

export const OFFICIAL_NOTE_SILENCE_MS = 10_000;
export const OFFICIAL_NOTE_CONFIRMATION_QUESTION = "Да завършвам ли забележката?";
const LATEST_INPUT_MAX_LENGTH = 4_096;

type CaptureStage = "idle" | "waiting-for-silence" | "awaiting-confirmation" | "confirmed";

interface PendingNote {
  callId: string;
  arguments: Record<string, unknown>;
  continuation: string;
  confirmation: string;
}

export interface OfficialNoteTarget {
  patientId: string;
  tooth: string;
  existingTreatmentId: string | null;
}

export type NoteToolDecision =
  | { kind: "unrelated"; item: FunctionCallItem }
  | { kind: "wait" }
  | { kind: "capture-in-progress"; callId: string; output: string }
  | { kind: "ask-again"; callId: string; output: string }
  | { kind: "execute"; item: FunctionCallItem };

export type NoteTranscriptDecision = "none" | "restart-silence" | "confirmed" | {
  kind: "finish";
  callId: string;
  output: string;
};

export class OfficialNoteDictation {
  private stage: CaptureStage = "idle";
  private pending: PendingNote | null = null;
  private latestInputTurn = "";

  beginInputTurn() {
    this.latestInputTurn = "";
  }

  intercept(item: FunctionCallItem): NoteToolDecision {
    if (item.name !== "write_aidoo_official_note") {
      this.latestInputTurn = "";
      return { kind: "unrelated", item };
    }
    if (!item.call_id || typeof item.arguments !== "string") return { kind: "unrelated", item };

    if (this.stage === "confirmed" && this.pending) {
      const note = combinedNote(this.pending);
      const argumentsToSave = { ...this.pending.arguments, note };
      this.reset();
      return {
        kind: "execute",
        item: { ...item, arguments: JSON.stringify(argumentsToSave) },
      };
    }

    if (this.stage === "awaiting-confirmation" && this.pending) {
      return {
        kind: "ask-again",
        callId: item.call_id,
        output: confirmationOutput(combinedNote(this.pending)),
      };
    }

    if (this.stage === "waiting-for-silence" && this.pending) {
      return {
        kind: "capture-in-progress",
        callId: item.call_id,
        output: captureInProgressOutput(combinedNote(this.pending)),
      };
    }

    const parsed = parseArguments(item.arguments);
    if (typeof parsed.note !== "string" || !parsed.note.trim()) {
      return { kind: "unrelated", item };
    }
    const ending = withoutEndingPhrase(parsed.note);
    const rawEnding = withoutEndingPhrase(this.latestInputTurn);
    if ((ending.finished && ending.note) || rawEnding.finished) {
      const note = ending.finished ? ending.note : parsed.note.trim();
      this.reset();
      return {
        kind: "execute",
        item: { ...item, arguments: JSON.stringify({ ...parsed, note }) },
      };
    }
    this.latestInputTurn = "";
    this.pending = {
      callId: item.call_id,
      arguments: parsed,
      continuation: "",
      confirmation: "",
    };
    this.stage = "waiting-for-silence";
    return { kind: "wait" };
  }

  pushTranscript(delta: string): NoteTranscriptDecision {
    if (!delta) return "none";
    this.latestInputTurn = `${this.latestInputTurn}${delta}`.slice(-LATEST_INPUT_MAX_LENGTH);
    if (!this.pending) return "none";
    if (this.stage === "waiting-for-silence") {
      this.pending.continuation += delta;
      const ending = withoutEndingPhrase(combinedNote(this.pending));
      if (ending.finished && ending.note) {
        this.pending.arguments.note = ending.note;
        this.pending.continuation = "";
        this.stage = "confirmed";
        return {
          kind: "finish",
          callId: this.pending.callId,
          output: readyToSaveOutput(ending.note),
        };
      }
      return "restart-silence";
    }
    if (this.stage !== "awaiting-confirmation") return "none";

    this.pending.confirmation += delta;
    const answer = normalized(this.pending.confirmation);
    if (isNegative(answer)) {
      this.pending.confirmation = "";
      this.stage = "waiting-for-silence";
      return "restart-silence";
    }
    if (isAffirmative(answer)) {
      this.stage = "confirmed";
      return "confirmed";
    }
    return "none";
  }

  finishSilence(): { callId: string; output: string } | null {
    if (this.stage !== "waiting-for-silence" || !this.pending) return null;
    this.stage = "awaiting-confirmation";
    return {
      callId: this.pending.callId,
      output: confirmationOutput(combinedNote(this.pending)),
    };
  }

  reset() {
    this.stage = "idle";
    this.pending = null;
    this.latestInputTurn = "";
  }

  currentText(): string | null {
    if (!this.pending) return null;
    return withoutEndingPhrase(combinedNote(this.pending)).note || null;
  }

  currentTarget(): OfficialNoteTarget | null {
    if (!this.pending) return null;
    const { patientId, tooth, existingTreatmentId } = this.pending.arguments;
    if (typeof patientId !== "string" || !patientId.trim()) return null;
    if (typeof tooth !== "string" || !tooth.trim()) return null;
    if (existingTreatmentId !== null
      && (typeof existingTreatmentId !== "string" || !existingTreatmentId.trim())) {
      return null;
    }
    return Object.freeze({ patientId, tooth, existingTreatmentId });
  }
}

function parseArguments(value: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(value) as unknown;
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : {};
  } catch {
    return {};
  }
}

function combinedNote(pending: PendingNote): string {
  const initial = String(pending.arguments.note ?? "").trim();
  const continuation = pending.continuation.trim();
  return continuation ? `${initial} ${continuation}` : initial;
}

function confirmationOutput(note: string): string {
  return JSON.stringify({
    ok: true,
    result: {
      saved: false,
      confirmationRequired: true,
      note,
      spokenSummary: OFFICIAL_NOTE_CONFIRMATION_QUESTION,
    },
  });
}

function captureInProgressOutput(note: string): string {
  return JSON.stringify({
    ok: true,
    result: {
      saved: false,
      captureInProgress: true,
      note,
      spokenSummary: "",
    },
  });
}

function readyToSaveOutput(note: string): string {
  return JSON.stringify({
    ok: true,
    result: {
      saved: false,
      confirmationRequired: false,
      readyToSave: true,
      note,
      spokenSummary: "",
    },
  });
}

function withoutEndingPhrase(value: string): { note: string; finished: boolean } {
  const phrase = "(?:готово|край\\s+на\\s+(?:забележката|бележката)|това\\s+е\\s+всичко|приключих|завърших)";
  const ending = new RegExp(`(^|[\\s,;:–—-])(?:${phrase}[\\s.!?,;:…–—-]*)+$`, "iu");
  const match = ending.exec(value.trim());
  if (!match) return { note: value.trim(), finished: false };
  return { note: value.trim().slice(0, match.index).trim(), finished: true };
}

function normalized(value: string): string {
  return value
    .toLocaleLowerCase("bg-BG")
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

function isAffirmative(value: string): boolean {
  return /^(да|да завършвай|завършвай|да завършена е|завършена е|готово|приключих|това е|край на (?:забележката|бележката))(?:\s|$)/u.test(value);
}

function isNegative(value: string): boolean {
  return /^(не|не още|не е завършена)(?:\s|$)/u.test(value);
}
