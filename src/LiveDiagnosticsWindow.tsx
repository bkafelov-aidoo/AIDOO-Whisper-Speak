import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { Check, Clipboard } from "lucide-react";
import { translator } from "./i18n";
import type { LiveDiagnosticEntry, LiveDiagnosticKind } from "./lib/live-diagnostics";
import type { AppLanguage } from "./types";

const KIND_KEYS: Record<LiveDiagnosticKind, Parameters<ReturnType<typeof translator>>[0]> = {
  heard: "liveDiagnosticsHeard",
  assistant: "liveDiagnosticsAssistant",
  backend: "liveDiagnosticsBackend",
  "tool-call": "liveDiagnosticsToolCall",
  "tool-result": "liveDiagnosticsToolResult",
  system: "liveDiagnosticsSystem",
  error: "liveDiagnosticsError",
};

export default function LiveDiagnosticsWindow() {
  const initialLanguage = new URLSearchParams(window.location.search).get("lang") === "en" ? "en" : "bg";
  const [language, setLanguage] = useState<AppLanguage>(initialLanguage);
  const t = translator(language);
  const [entries, setEntries] = useState<LiveDiagnosticEntry[]>([]);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const register = (subscription: Promise<() => void>, after?: () => void) => {
      void subscription.then((dispose) => {
        if (disposed) dispose();
        else {
          unlisteners.push(dispose);
          after?.();
        }
      });
    };
    register(listen<LiveDiagnosticEntry[]>("live-diagnostics:updated", ({ payload }) => {
      setEntries(Array.isArray(payload) ? payload : []);
    }), () => {
      void emitTo("main", "live-diagnostics:ready").catch(() => undefined);
    });
    register(listen<AppLanguage>("live-diagnostics:language", ({ payload }) => {
      if (payload === "bg" || payload === "en") setLanguage(payload);
    }));
    return () => {
      disposed = true;
      unlisteners.forEach((dispose) => dispose());
    };
  }, []);

  const plainText = useMemo(() => entries.map((entry) => {
    const time = new Date(entry.at).toLocaleTimeString(language === "bg" ? "bg-BG" : "en-GB", { hour12: false });
    return `[${time}] ${t(KIND_KEYS[entry.kind])}\n${entry.detail}`;
  }).join("\n\n"), [entries, language, t]);

  const copyAll = async () => {
    await invoke("copy_text", { text: plainText });
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1_500);
  };

  return <main className="live-diagnostics-window">
    <header>
      <h1>{t("liveDiagnosticsTitle")}</h1>
      <button className="secondary-button" disabled={!entries.length} onClick={() => void copyAll()}>
        {copied ? <Check /> : <Clipboard />}{copied ? t("copied") : t("copy")}
      </button>
    </header>
    {entries.length === 0
      ? <div className="live-diagnostics-empty"><strong>{t("liveDiagnosticsEmpty")}</strong><p>{t("liveDiagnosticsEmptyHelp")}</p></div>
      : <ol className="live-diagnostics-list">{entries.map((entry) => <li key={entry.id} className={entry.kind}>
        <div><strong>{t(KIND_KEYS[entry.kind])}</strong><time>{new Date(entry.at).toLocaleTimeString(language === "bg" ? "bg-BG" : "en-GB", { hour12: false })}</time></div>
        <pre>{entry.detail}</pre>
      </li>)}</ol>}
  </main>;
}
