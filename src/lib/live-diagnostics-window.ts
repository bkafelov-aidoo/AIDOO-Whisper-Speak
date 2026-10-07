import { invoke } from "@tauri-apps/api/core";
import type { AppLanguage } from "../types";

export async function openLiveDiagnosticsWindow(language: AppLanguage) {
  await invoke("open_live_diagnostics", { language });
}
