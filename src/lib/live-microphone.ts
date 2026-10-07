export interface MicrophoneAcquireOptions {
  attemptTimeoutMs: number;
  firstAttemptTimeoutMs?: number;
  retryDelayMs: number;
  timeoutMessage?: string;
  attemptObserver?: (event: MicrophoneAttemptEvent) => void;
  shouldRetry?: () => boolean;
}

export type MicrophoneAttemptState = "started" | "succeeded" | "timed-out" | "rejected";

export interface MicrophoneAttemptEvent {
  attempt: number;
  state: MicrophoneAttemptState;
  elapsedMs: number;
}

interface StoppableMediaStream {
  getTracks(): Array<{ stop(): void }>;
}

class MediaRequestTimeoutError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "MediaRequestTimeoutError";
  }
}

const COLD_ATTEMPT_TIMEOUT_MS = 7_000;
const COLD_RETRY_DELAY_MS = 300;
const WARM_FIRST_ATTEMPT_TIMEOUT_MS = 1_500;
const WARM_RETRY_DELAY_MS = 100;

export function microphoneAcquireOptionsForDevice(
  currentDeviceName: string | null,
  priorSuccessfulDeviceName: string | null | undefined,
): MicrophoneAcquireOptions {
  const warm = priorSuccessfulDeviceName !== undefined
    && priorSuccessfulDeviceName === currentDeviceName;
  return {
    attemptTimeoutMs: COLD_ATTEMPT_TIMEOUT_MS,
    firstAttemptTimeoutMs: warm
      ? WARM_FIRST_ATTEMPT_TIMEOUT_MS
      : COLD_ATTEMPT_TIMEOUT_MS,
    retryDelayMs: warm ? WARM_RETRY_DELAY_MS : COLD_RETRY_DELAY_MS,
  };
}

function observeAttempt(
  observer: MicrophoneAcquireOptions["attemptObserver"],
  attempt: number,
  state: MicrophoneAttemptState,
  startedAt: number,
) {
  if (!observer) return;
  const atMs = performance.now();
  try {
    observer({
      attempt,
      state,
      elapsedMs: Math.max(0, atMs - startedAt),
    });
  } catch {
    // Diagnostics must never influence microphone permission, retry, or stream ownership.
  }
}

function retryAllowed(predicate: MicrophoneAcquireOptions["shouldRetry"]): boolean {
  if (!predicate) return true;
  try {
    return predicate();
  } catch {
    return false;
  }
}

async function requestMicrophone<T extends StoppableMediaStream>(
  request: (constraints: MediaStreamConstraints) => Promise<T>,
  constraints: MediaStreamConstraints,
  timeoutMs: number,
  message: string,
  attempt: number,
  observer: MicrophoneAcquireOptions["attemptObserver"],
): Promise<T> {
  const startedAt = performance.now();
  observeAttempt(observer, attempt, "started", startedAt);
  try {
    const stream = await mediaRequestWithTimeout(request(constraints), timeoutMs, message);
    observeAttempt(observer, attempt, "succeeded", startedAt);
    return stream;
  } catch (reason) {
    observeAttempt(
      observer,
      attempt,
      reason instanceof MediaRequestTimeoutError ? "timed-out" : "rejected",
      startedAt,
    );
    throw reason;
  }
}

export function mediaRequestWithTimeout<T extends StoppableMediaStream>(
  request: Promise<T>,
  timeoutMs: number,
  message: string,
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    let finished = false;
    const timeout = setTimeout(() => {
      if (finished) return;
      finished = true;
      reject(new MediaRequestTimeoutError(message));
    }, timeoutMs);

    request.then(
      (stream) => {
        if (finished) {
          stream.getTracks().forEach((track) => track.stop());
          return;
        }
        finished = true;
        clearTimeout(timeout);
        resolve(stream);
      },
      (reason) => {
        if (finished) return;
        finished = true;
        clearTimeout(timeout);
        reject(reason);
      },
    );
  });
}

export async function acquireMicrophone<T extends StoppableMediaStream>(
  request: (constraints: MediaStreamConstraints) => Promise<T>,
  constraints: MediaStreamConstraints,
  options: MicrophoneAcquireOptions,
): Promise<T> {
  const message = options.timeoutMessage ?? "Микрофонът не отговори навреме.";
  let firstTimeout: MediaRequestTimeoutError;
  try {
    return await requestMicrophone(
      request,
      constraints,
      options.firstAttemptTimeoutMs ?? options.attemptTimeoutMs,
      message,
      1,
      options.attemptObserver,
    );
  } catch (reason) {
    if (!(reason instanceof MediaRequestTimeoutError)) throw reason;
    firstTimeout = reason;
  }

  if (!retryAllowed(options.shouldRetry)) throw firstTimeout;

  if (options.retryDelayMs > 0) {
    await new Promise<void>((resolve) => setTimeout(resolve, options.retryDelayMs));
  }
  if (!retryAllowed(options.shouldRetry)) throw firstTimeout;
  return requestMicrophone(
    request,
    constraints,
    options.attemptTimeoutMs,
    message,
    2,
    options.attemptObserver,
  );
}
