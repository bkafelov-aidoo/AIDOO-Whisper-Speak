export type LiveStartupStage =
  | "native-preparation"
  | "microphone-handoff"
  | "device-selection"
  | "microphone-acquisition"
  | "webrtc-offer"
  | "ice-gathering"
  | "session-request"
  | "remote-description"
  | "session-ready";

export interface LiveStartupMeasurement {
  stage: LiveStartupStage;
  outcome: "completed" | "failed";
  elapsedMs: number;
  totalElapsedMs: number;
}

const STAGE_LABELS: Record<LiveStartupStage, string> = {
  "native-preparation": "подготовка на приложението",
  "microphone-handoff": "освобождаване на микрофона",
  "device-selection": "избор на микрофон",
  "microphone-acquisition": "включване на микрофона",
  "webrtc-offer": "подготовка на гласовата връзка",
  "ice-gathering": "подготовка на мрежовата връзка",
  "session-request": "заявка за гласова сесия",
  "remote-description": "прилагане на отговора",
  "session-ready": "готовност за слушане",
};

export function describeLiveStartupMeasurement(value: LiveStartupMeasurement): string {
  return `Старт — ${STAGE_LABELS[value.stage]}: ${value.elapsedMs} ms; `
    + `общо ${value.totalElapsedMs} ms; ${value.outcome === "completed" ? "готово" : "неуспешно"}.`;
}

/** Observes elapsed time only: never stores action arguments, results or errors. */
export class LiveStartupTiming {
  private readonly startedAt: number;
  private readyReported = false;
  private readonly observer?: (measurement: LiveStartupMeasurement) => void;
  private readonly now: () => number;

  constructor(
    observer?: (measurement: LiveStartupMeasurement) => void,
    now: () => number = () => performance.now(),
  ) {
    this.observer = observer;
    this.now = now;
    this.startedAt = now();
  }

  async measure<T>(stage: LiveStartupStage, action: () => Promise<T>): Promise<T> {
    const started = this.now();
    try {
      const result = await action();
      this.report(stage, started, "completed");
      return result;
    } catch (reason) {
      this.report(stage, started, "failed");
      throw reason;
    }
  }

  ready(): void {
    if (this.readyReported) return;
    this.readyReported = true;
    this.report("session-ready", this.startedAt, "completed");
  }

  private report(stage: LiveStartupStage, started: number, outcome: LiveStartupMeasurement["outcome"]): void {
    if (!this.observer) return;
    const finished = this.now();
    try {
      this.observer({
        stage,
        outcome,
        elapsedMs: Math.max(0, Math.round(finished - started)),
        totalElapsedMs: Math.max(0, Math.round(finished - this.startedAt)),
      });
    } catch {
      // Timing must never delay, fail or repeat a microphone/network operation.
    }
  }
}
