export const MAX_LIVE_DIAGNOSTIC_ENTRIES = 120;
export const MAX_LIVE_DIAGNOSTIC_DETAIL_CHARS = 4_000;

export type LiveDiagnosticKind =
  | "heard"
  | "assistant"
  | "backend"
  | "tool-call"
  | "tool-result"
  | "system"
  | "error";

export interface LiveDiagnosticEntry {
  id: string;
  at: number;
  kind: LiveDiagnosticKind;
  detail: string;
}

interface NewLiveDiagnosticEntry {
  at?: number;
  kind: LiveDiagnosticKind;
  detail: string;
}

let diagnosticSequence = 0;

export function appendLiveDiagnostic(
  entries: readonly LiveDiagnosticEntry[],
  entry: NewLiveDiagnosticEntry,
): LiveDiagnosticEntry[] {
  const at = entry.at ?? Date.now();
  return boundEntries([...entries, {
    id: `${at}-${diagnosticSequence += 1}`,
    at,
    kind: entry.kind,
    detail: boundDetail(entry.detail),
  }]);
}

export function appendLiveTranscriptDelta(
  entries: readonly LiveDiagnosticEntry[],
  kind: "heard" | "assistant" | "backend",
  delta: string,
  at = Date.now(),
): LiveDiagnosticEntry[] {
  if (!delta) return [...entries];
  const last = entries[entries.length - 1];
  if (last?.kind !== kind) return appendLiveDiagnostic(entries, { kind, detail: delta, at });
  const updated = {
    ...last,
    at,
    detail: boundDetail(`${last.detail}${delta}`),
  };
  return boundEntries([...entries.slice(0, -1), updated]);
}

export function describeLiveToolCall(name: string | undefined, rawArguments: string | undefined) {
  return describeToolPayload(name, rawArguments);
}

export function describeLiveToolResult(name: string | undefined, rawOutput: string | undefined) {
  return describeToolPayload(name, rawOutput);
}

function describeToolPayload(name: string | undefined, rawValue: string | undefined) {
  const toolName = name?.trim() || "unknown_tool";
  if (!rawValue) return toolName;
  let detail = rawValue;
  try {
    detail = JSON.stringify(JSON.parse(rawValue), null, 2);
  } catch {
    // Keep malformed payloads visible; they are often the most useful evidence.
  }
  return boundDetail(`${toolName}\n${detail}`);
}

function boundEntries(entries: LiveDiagnosticEntry[]) {
  return entries.slice(-MAX_LIVE_DIAGNOSTIC_ENTRIES);
}

function boundDetail(value: string) {
  if (value.length <= MAX_LIVE_DIAGNOSTIC_DETAIL_CHARS) return value;
  return `${value.slice(0, MAX_LIVE_DIAGNOSTIC_DETAIL_CHARS - 14)}\n…[съкратено]`;
}
