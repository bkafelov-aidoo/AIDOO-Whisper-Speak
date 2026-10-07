import { useCallback, useEffect, useRef, useState, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import {
  ASSISTANT_CLOSE_GRACE_MS,
  AssistantVoiceCommandDetector,
  detectAssistantVoiceCommandFromLiveEvent,
} from "../lib/assistant-command";
import { backendUsageFromLiveEvent, executeAidooLiveTool, functionCallFromLiveEvent, sendAidooToolOutput } from "../lib/aidoo-live-tools";
import {
  ASSISTANT_GOODBYE_SILENCE_MS,
  ASSISTANT_GOODBYE_START_TIMEOUT_MS,
  ASSISTANT_UNRECOGNIZED_SPEECH_MS,
  LiveInactivityTimer,
  assistantClarificationInstruction,
  assistantClarificationPrompt,
  assistantGreetingInstruction,
  assistantGreetingPrompt,
  assistantGoodbyeInstruction,
  assistantGoodbyePrompt,
  inputSpeechState,
} from "../lib/live-inactivity";
import { acquireMicrophone, microphoneAcquireOptionsForDevice } from "../lib/live-microphone";
import { LiveStartupTiming, describeLiveStartupMeasurement } from "../lib/live-startup-timing";
import { consumePendingRequest, createEventScope } from "../lib/event-scope";
import {
  OFFICIAL_NOTE_SILENCE_MS,
  OfficialNoteDictation,
} from "../lib/official-note-dictation";
import {
  appendLiveDiagnostic,
  appendLiveTranscriptDelta,
  describeLiveToolCall,
  describeLiveToolResult,
  type LiveDiagnosticEntry,
  type LiveDiagnosticKind,
} from "../lib/live-diagnostics";

export type LivePhase = "idle" | "preparing" | "connecting" | "listening" | "speaking" | "working" | "switching" | "closing" | "error";

export interface LiveConversationState {
  phase: LivePhase;
  error: string | null;
  diagnostics: LiveDiagnosticEntry[];
  start: () => Promise<void>;
  stop: () => void;
}

interface LiveSessionAnswer {
  sessionId: string;
  sdp: string;
}

interface LiveEvent {
  type?: string;
  client_event_id?: string;
  delta?: string;
  start_ms?: number;
  end_ms?: number;
  error?: { message?: string };
  event?: {
    type?: string;
    delta?: string;
    text?: string;
    item?: { type?: string; call_id?: string; name?: string; arguments?: string };
    response?: {
      id?: string;
      model?: string;
      status?: string;
      error?: { message?: string };
      incomplete_details?: { reason?: string };
      usage?: {
        input_tokens?: number;
        input_tokens_details?: { cached_tokens?: number; cache_write_tokens?: number };
        output_tokens?: number;
      };
    };
  };
}

const ICE_GATHERING_TIMEOUT_MS = 10_000;
const LIVE_CREATE_TIMEOUT_MS = 50_000;
const SESSION_START_TIMEOUT_MS = 20_000;

export function useLiveConversation(
  microphoneName: string | null,
  onError: (reason: unknown) => void,
  onDictationStarted?: () => void,
  onAssistantRequested?: () => void,
  diagnosticsEnabled = false,
): LiveConversationState {
  const [phase, setPhase] = useState<LivePhase>("idle");
  const [error, setError] = useState<string | null>(null);
  const [diagnostics, setDiagnostics] = useState<LiveDiagnosticEntry[]>([]);
  const diagnosticsRef = useRef<LiveDiagnosticEntry[]>([]);
  diagnosticsRef.current = diagnostics;
  const peerRef = useRef<RTCPeerConnection | null>(null);
  const channelRef = useRef<RTCDataChannel | null>(null);
  const microphoneRef = useRef<MediaStream | null>(null);
  const previousSuccessfulMicrophoneRef = useRef<string | null | undefined>(undefined);
  const microphoneDeviceRevisionRef = useRef(0);
  const audioContextRef = useRef<AudioContext | null>(null);
  const monitorFrameRef = useRef<number | null>(null);
  const timeoutRef = useRef<number | null>(null);
  const goodbyeTimerRef = useRef<number | null>(null);
  const clarificationTimerRef = useRef<number | null>(null);
  const recognitionTimerRef = useRef<number | null>(null);
  const officialNoteTimerRef = useRef<number | null>(null);
  const greetingInstructionIdRef = useRef<string | null>(null);
  const clarificationInstructionIdRef = useRef<string | null>(null);
  const goodbyeInstructionIdRef = useRef<string | null>(null);
  const readyRef = useRef(false);
  const closingRef = useRef(false);
  const clarificationActiveRef = useRef(false);
  const speechRecognizedRef = useRef(false);
  const switchingRef = useRef(false);
  const toolBusyRef = useRef(false);
  const mountedRef = useRef(true);
  const operationRef = useRef(0);
  const stopRef = useRef<() => void>(() => undefined);
  const voiceCloseRef = useRef<() => void>(() => undefined);
  const commandDetectorRef = useRef(new AssistantVoiceCommandDetector({
    onDeferredCommand: (command) => {
      if (command === "end-session") voiceCloseRef.current();
    },
  }));
  const startRef = useRef<() => Promise<void>>(async () => undefined);
  const inactivityHandlerRef = useRef<() => void>(() => undefined);
  const inactivityFeedbackHandlerRef = useRef<() => void>(() => undefined);
  const inactivityTimerRef = useRef<LiveInactivityTimer | null>(null);
  const handledToolCallsRef = useRef(new Set<string>());
  const officialNoteRef = useRef(new OfficialNoteDictation());
  const officialNotePreviewRevisionRef = useRef(0);
  if (inactivityTimerRef.current === null) {
    inactivityTimerRef.current = new LiveInactivityTimer(
      () => inactivityFeedbackHandlerRef.current(),
      () => inactivityHandlerRef.current(),
    );
  }

  const updatePhase = useCallback((next: LivePhase) => {
    if (mountedRef.current) setPhase(next);
  }, []);

  const addDiagnostic = useCallback((kind: LiveDiagnosticKind, detail: string) => {
    if (!diagnosticsEnabled || !detail) return;
    setDiagnostics((current) => appendLiveDiagnostic(current, { kind, detail }));
  }, [diagnosticsEnabled]);

  const addTranscriptDelta = useCallback((kind: "heard" | "assistant" | "backend", delta: string) => {
    if (!diagnosticsEnabled || !delta) return;
    setDiagnostics((current) => appendLiveTranscriptDelta(current, kind, delta));
  }, [diagnosticsEnabled]);

  const publishOfficialNotePreview = useCallback((text: string | null) => {
    const revision = ++officialNotePreviewRevisionRef.current;
    if (!text) {
      void emitTo("overlay", "assistant:note-transcript", { active: false, text: "" }).catch(() => undefined);
      return;
    }
    const target = officialNoteRef.current.currentTarget();
    const preview = target
      ? invoke<boolean>("aidoo_preview_official_note", { text, patientId: target.patientId })
      : Promise.resolve(false);
    void preview.then((shownInAidoo) => {
      if (revision !== officialNotePreviewRevisionRef.current) return;
      return emitTo("overlay", "assistant:note-transcript", {
        active: !shownInAidoo,
        text: shownInAidoo ? "" : text,
      });
    }).catch(() => {
      if (revision !== officialNotePreviewRevisionRef.current) return;
      void emitTo("overlay", "assistant:note-transcript", { active: true, text }).catch(() => undefined);
    });
  }, []);

  useEffect(() => {
    void invoke("set_live_phase", { phase }).catch(() => undefined);
  }, [phase]);

  useEffect(() => {
    const devices = navigator.mediaDevices;
    if (!devices) return;
    const invalidateMicrophoneProof = () => {
      microphoneDeviceRevisionRef.current += 1;
      previousSuccessfulMicrophoneRef.current = undefined;
    };
    devices.addEventListener("devicechange", invalidateMicrophoneProof);
    return () => devices.removeEventListener("devicechange", invalidateMicrophoneProof);
  }, []);

  useEffect(() => {
    microphoneDeviceRevisionRef.current += 1;
    previousSuccessfulMicrophoneRef.current = undefined;
  }, [microphoneName]);

  useEffect(() => {
    if (!diagnosticsEnabled) return;
    void emitTo("live-diagnostics", "live-diagnostics:updated", diagnostics).catch(() => undefined);
  }, [diagnostics, diagnosticsEnabled]);

  useEffect(() => {
    if (!diagnosticsEnabled) return;
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    const register = (subscription: Promise<() => void>) => {
      void subscription.then((dispose) => {
        if (disposed) dispose(); else unlisteners.push(dispose);
      });
    };
    register(listen("live-diagnostics:ready", () => {
      void emitTo("live-diagnostics", "live-diagnostics:updated", diagnosticsRef.current).catch(() => undefined);
    }));
    return () => {
      disposed = true;
      unlisteners.forEach((dispose) => dispose());
    };
  }, [diagnosticsEnabled]);

  const clearTimer = useCallback(() => {
    if (timeoutRef.current !== null) window.clearTimeout(timeoutRef.current);
    timeoutRef.current = null;
  }, []);

  const clearGoodbyeTimer = useCallback(() => {
    if (goodbyeTimerRef.current !== null) window.clearTimeout(goodbyeTimerRef.current);
    goodbyeTimerRef.current = null;
  }, []);

  const clearClarificationTimer = useCallback(() => {
    if (clarificationTimerRef.current !== null) window.clearTimeout(clarificationTimerRef.current);
    clarificationTimerRef.current = null;
  }, []);

  const clearRecognitionTimer = useCallback(() => {
    if (recognitionTimerRef.current !== null) window.clearTimeout(recognitionTimerRef.current);
    recognitionTimerRef.current = null;
  }, []);

  const clearOfficialNoteTimer = useCallback(() => {
    if (officialNoteTimerRef.current !== null) window.clearTimeout(officialNoteTimerRef.current);
    officialNoteTimerRef.current = null;
  }, []);

  const finishOfficialNoteSilence = useCallback(() => {
    clearOfficialNoteTimer();
    const result = officialNoteRef.current.finishSilence();
    const channel = channelRef.current;
    if (!result || !channel || channel.readyState !== "open" || closingRef.current) return;
    addDiagnostic("system", "Забележката остана без продължение 10 секунди. Изчаква се потвърждение.");
    sendAidooToolOutput(channel, result.callId, result.output);
    updatePhase("listening");
    inactivityTimerRef.current?.start();
  }, [addDiagnostic, clearOfficialNoteTimer, updatePhase]);

  const armOfficialNoteTimer = useCallback(() => {
    clearOfficialNoteTimer();
    officialNoteTimerRef.current = window.setTimeout(
      finishOfficialNoteSilence,
      OFFICIAL_NOTE_SILENCE_MS,
    );
  }, [clearOfficialNoteTimer, finishOfficialNoteSilence]);

  const releaseBrowserMedia = useCallback(() => {
    clearTimer();
    clearGoodbyeTimer();
    clearClarificationTimer();
    clearRecognitionTimer();
    clearOfficialNoteTimer();
    inactivityTimerRef.current?.stop();
    readyRef.current = false;
    if (monitorFrameRef.current !== null) cancelAnimationFrame(monitorFrameRef.current);
    monitorFrameRef.current = null;
    microphoneRef.current?.getTracks().forEach((track) => track.stop());
    microphoneRef.current = null;
    channelRef.current?.close();
    channelRef.current = null;
    peerRef.current?.close();
    peerRef.current = null;
    void audioContextRef.current?.close().catch(() => undefined);
    audioContextRef.current = null;
    commandDetectorRef.current.reset();
    handledToolCallsRef.current.clear();
    officialNoteRef.current.reset();
    publishOfficialNotePreview(null);
    toolBusyRef.current = false;
    greetingInstructionIdRef.current = null;
    clarificationInstructionIdRef.current = null;
    goodbyeInstructionIdRef.current = null;
    clarificationActiveRef.current = false;
    speechRecognizedRef.current = false;
  }, [clearClarificationTimer, clearGoodbyeTimer, clearOfficialNoteTimer, clearRecognitionTimer, clearTimer, publishOfficialNotePreview]);

  const finish = useCallback((nextPhase: LivePhase = "idle") => {
    operationRef.current += 1;
    closingRef.current = true;
    switchingRef.current = false;
    releaseBrowserMedia();
    void invoke("end_live_session").catch(() => undefined);
    updatePhase(nextPhase);
  }, [releaseBrowserMedia, updatePhase]);

  const fail = useCallback((reason: unknown) => {
    const message = String(reason).replace(/^Error:\s*/, "");
    addDiagnostic("error", message);
    operationRef.current += 1;
    closingRef.current = true;
    switchingRef.current = false;
    releaseBrowserMedia();
    void invoke("end_live_session").catch(() => undefined);
    if (mountedRef.current) {
      setError(message);
      setPhase("error");
      onError(reason);
    }
  }, [addDiagnostic, onError, releaseBrowserMedia]);

  const switchToDictation = useCallback(async () => {
    if (switchingRef.current) return;
    switchingRef.current = true;
    closingRef.current = true;
    operationRef.current += 1;
    updatePhase("switching");
    releaseBrowserMedia();
    try {
      await invoke("end_live_session");
      await invoke("start_voice_dictation");
      switchingRef.current = false;
      updatePhase("idle");
      onDictationStarted?.();
    } catch (reason) {
      fail(reason);
    }
  }, [fail, onDictationStarted, releaseBrowserMedia, updatePhase]);

  const stop = useCallback(() => {
    if (closingRef.current) return;
    operationRef.current += 1;
    closingRef.current = true;
    updatePhase("closing");
    microphoneRef.current?.getTracks().forEach((track) => track.stop());
    microphoneRef.current = null;
    const channel = channelRef.current;
    if (readyRef.current && channel?.readyState === "open") {
      try {
        channel.send(JSON.stringify({ type: "session.close" }));
      } catch {
        finish("idle");
        return;
      }
      clearTimer();
      timeoutRef.current = window.setTimeout(() => finish("idle"), ASSISTANT_CLOSE_GRACE_MS);
    } else {
      finish("idle");
    }
  }, [clearTimer, finish, updatePhase]);
  stopRef.current = stop;

  const requestVoiceClose = useCallback(() => {
    if (officialNoteRef.current.currentText()) {
      const message = "Забележката не е записана.";
      addDiagnostic("system", message);
      void emitTo("overlay", "toast", message).catch(() => undefined);
    }
    stopRef.current();
  }, [addDiagnostic]);
  voiceCloseRef.current = requestVoiceClose;

  const finishAfterLocalGoodbye = useCallback(() => {
    clearGoodbyeTimer();
    const synth = window.speechSynthesis;
    if (!synth || typeof SpeechSynthesisUtterance === "undefined") {
      finish("idle");
      return;
    }
    const utterance = new SpeechSynthesisUtterance("Чао!");
    utterance.lang = "bg-BG";
    utterance.rate = 0.95;
    let completed = false;
    const complete = () => {
      if (completed) return;
      completed = true;
      clearGoodbyeTimer();
      finish("idle");
    };
    utterance.onend = complete;
    utterance.onerror = complete;
    goodbyeTimerRef.current = window.setTimeout(complete, 2_500);
    try {
      synth.speak(utterance);
    } catch {
      complete();
    }
  }, [clearGoodbyeTimer, finish]);

  const speakClarificationLocally = useCallback(() => {
    clearClarificationTimer();
    if (!clarificationActiveRef.current || closingRef.current) return;
    const synth = window.speechSynthesis;
    if (!synth || typeof SpeechSynthesisUtterance === "undefined") return;
    const utterance = new SpeechSynthesisUtterance("Не ви чух добре. Моля, повторете.");
    utterance.lang = "bg-BG";
    utterance.rate = 0.95;
    try { synth.speak(utterance); } catch { /* The session remains open for another attempt. */ }
  }, [clearClarificationTimer]);

  const beginInactivityFeedback = useCallback(() => {
    if (closingRef.current) return;
    if (toolBusyRef.current) {
      inactivityTimerRef.current?.start();
      return;
    }
    clarificationActiveRef.current = true;
    addDiagnostic("system", "Няма разбираема активност 30 секунди. Асистентът иска повторение и остава активен.");
    const channel = channelRef.current;
    if (!readyRef.current || channel?.readyState !== "open") {
      speakClarificationLocally();
      return;
    }
    const eventId = `aidoo_idle_clarification_${Date.now()}`;
    clarificationInstructionIdRef.current = eventId;
    try {
      channel.send(JSON.stringify(assistantClarificationInstruction(eventId)));
      clearClarificationTimer();
      clarificationTimerRef.current = window.setTimeout(speakClarificationLocally, 5_000);
    } catch {
      clarificationInstructionIdRef.current = null;
      speakClarificationLocally();
    }
  }, [addDiagnostic, clearClarificationTimer, speakClarificationLocally]);
  inactivityFeedbackHandlerRef.current = beginInactivityFeedback;

  const beginInactiveClose = useCallback(() => {
    if (closingRef.current) return;
    const channel = channelRef.current;
    if (toolBusyRef.current) {
      inactivityTimerRef.current?.start();
      return;
    }
    if (!readyRef.current || channel?.readyState !== "open") {
      finish("idle");
      return;
    }
    clarificationActiveRef.current = false;
    clarificationInstructionIdRef.current = null;
    clearClarificationTimer();
    closingRef.current = true;
    updatePhase("closing");
    const eventId = `aidoo_idle_goodbye_${Date.now()}`;
    goodbyeInstructionIdRef.current = eventId;
    try {
      channel.send(JSON.stringify(assistantGoodbyeInstruction(eventId)));
    } catch {
      finishAfterLocalGoodbye();
      return;
    }
    clearGoodbyeTimer();
    goodbyeTimerRef.current = window.setTimeout(finishAfterLocalGoodbye, ASSISTANT_GOODBYE_START_TIMEOUT_MS);
  }, [clearClarificationTimer, clearGoodbyeTimer, finish, finishAfterLocalGoodbye, updatePhase]);
  inactivityHandlerRef.current = beginInactiveClose;

  const registerUserActivity = useCallback(() => {
    if (clarificationActiveRef.current) {
      window.speechSynthesis?.cancel();
    }
    clarificationActiveRef.current = false;
    clarificationInstructionIdRef.current = null;
    clearClarificationTimer();
    clearRecognitionTimer();
    inactivityTimerRef.current?.touch();
  }, [clearClarificationTimer, clearRecognitionTimer]);

  const registerRemoteSpeech = useCallback(() => {
    if (clarificationActiveRef.current) {
      clearClarificationTimer();
      return;
    }
    if (!closingRef.current || !goodbyeInstructionIdRef.current) {
      inactivityTimerRef.current?.touch();
      return;
    }
    clearGoodbyeTimer();
    goodbyeTimerRef.current = window.setTimeout(() => finish("idle"), ASSISTANT_GOODBYE_SILENCE_MS);
  }, [clearClarificationTimer, clearGoodbyeTimer, finish]);

  useEffect(() => {
    mountedRef.current = true;
    let active = true;
    const events = createEventScope(listen, (reason) => {
      if (active && mountedRef.current) onError(reason);
    });
    const consumeAssistantRequest = async () => {
      try {
        await consumePendingRequest(
          () => invoke<boolean>("take_assistant_request"),
          async () => {
            onAssistantRequested?.();
            await startRef.current();
          },
          () => active && mountedRef.current,
        );
      } catch (reason) {
        if (active && mountedRef.current) onError(reason);
      }
    };
    const onFocus = () => { void consumeAssistantRequest(); };
    events.listen<string>("live:force-close", () => finish("idle"));
    events.listen(
      "assistant:requested",
      () => { void consumeAssistantRequest(); },
      () => { void consumeAssistantRequest(); },
    );
    window.addEventListener("focus", onFocus);
    return () => {
      active = false;
      mountedRef.current = false;
      events.dispose();
      window.removeEventListener("focus", onFocus);
      operationRef.current += 1;
      releaseBrowserMedia();
      void invoke("end_live_session").catch(() => undefined);
    };
  }, [finish, onAssistantRequested, onError, releaseBrowserMedia]);

  const start = useCallback(async () => {
    if (!["idle", "error"].includes(phase)) return;
    const operation = operationRef.current + 1;
    operationRef.current = operation;
    const stillCurrent = () => operationRef.current === operation && mountedRef.current;
    setError(null);
    addDiagnostic("system", "Започва нова GPT-Live сесия.");
    updatePhase("preparing");
    closingRef.current = false;
    clarificationActiveRef.current = false;
    switchingRef.current = false;
    commandDetectorRef.current.reset();
    const microphoneDeviceRevision = microphoneDeviceRevisionRef.current;
    const startupTiming = new LiveStartupTiming(diagnosticsEnabled ? (measurement) => {
      if (stillCurrent()) addDiagnostic("system", describeLiveStartupMeasurement(measurement));
    } : undefined);
    try {
      await startupTiming.measure("native-preparation", () => invoke("prepare_live_session"));
      if (!stillCurrent()) return;
      await startupTiming.measure("microphone-handoff", () => delay(180));
      if (!stillCurrent()) return;

      const peer = new RTCPeerConnection();
      peerRef.current = peer;
      const audioConstraints: MediaTrackConstraints = {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      };
      let selectedMicrophoneResolved = !microphoneName;
      if (microphoneName) {
        const devices = await startupTiming.measure("device-selection", () => navigator.mediaDevices.enumerateDevices());
        const selected = devices.find((device) => device.kind === "audioinput" && device.label === microphoneName)
          ?? devices.find((device) => device.kind === "audioinput" && device.label.includes(microphoneName));
        if (selected?.deviceId) {
          audioConstraints.deviceId = { exact: selected.deviceId };
          selectedMicrophoneResolved = true;
        }
      }
      const microphone = await startupTiming.measure("microphone-acquisition", () => acquireMicrophone(
        (constraints) => navigator.mediaDevices.getUserMedia(constraints),
        { audio: audioConstraints },
        {
          ...microphoneAcquireOptionsForDevice(microphoneName,
            selectedMicrophoneResolved ? previousSuccessfulMicrophoneRef.current : undefined),
          shouldRetry: () => stillCurrent() && microphoneDeviceRevisionRef.current === microphoneDeviceRevision,
          attemptObserver: (event) => {
            if (stillCurrent() && diagnosticsEnabled) {
              addDiagnostic("system", `Микрофон — опит ${event.attempt}: ${event.state}; ${Math.round(event.elapsedMs)} ms.`);
            }
          },
        },
      )).catch((reason) => {
        if (stillCurrent()) previousSuccessfulMicrophoneRef.current = undefined;
        throw reason;
      });
      if (!stillCurrent()) {
        microphone.getTracks().forEach((track) => track.stop());
        return;
      }
      microphoneRef.current = microphone;
      const inputTracks = microphone.getAudioTracks();
      for (const track of inputTracks) peer.addTrack(track, microphone);
      const inputTrack = inputTracks[0];
      previousSuccessfulMicrophoneRef.current = selectedMicrophoneResolved && inputTrack?.readyState === "live"
        && microphoneDeviceRevisionRef.current === microphoneDeviceRevision ? microphoneName : undefined;
      if (inputTrack) {
        const settings = inputTrack.getSettings();
        addDiagnostic("system", `Микрофон: ${inputTrack.label || "системен"}\n${JSON.stringify({
          sampleRate: settings.sampleRate ?? null,
          channelCount: settings.channelCount ?? null,
          echoCancellation: settings.echoCancellation ?? null,
          noiseSuppression: settings.noiseSuppression ?? null,
          autoGainControl: settings.autoGainControl ?? null,
        }, null, 2)}`);
      }
      updatePhase("connecting");

      peer.addEventListener("track", ({ track }) => monitorRemoteAudio(track, audioContextRef, monitorFrameRef, readyRef, closingRef, toolBusyRef, mountedRef, updatePhase, registerRemoteSpeech));

      const channel = peer.createDataChannel("oai-events");
      channelRef.current = channel;
      channel.addEventListener("message", ({ data }) => {
        if (typeof data !== "string") return;
        let event: LiveEvent;
        try { event = JSON.parse(data) as LiveEvent; } catch { return; }
        const terminalEvent = event.type === "session.closed"
          || event.type === "error"
          || event.type === "session.failed";
        const goodbyeInstructionEvent = goodbyeInstructionIdRef.current !== null
          && event.type === "session.instructions.appended"
          && event.client_event_id === goodbyeInstructionIdRef.current;
        if (closingRef.current && !terminalEvent && !goodbyeInstructionEvent) return;
        const speechState = inputSpeechState(event.type);
        if (event.type === "session.started") {
          if (stillCurrent()) startupTiming.ready();
          clearTimer();
          readyRef.current = true;
          addDiagnostic("system", "GPT-Live сесията е готова и слуша.");
          updatePhase("listening");
          inactivityTimerRef.current?.start();
          const greetingId = `aidoo_start_greeting_${Date.now()}`;
          greetingInstructionIdRef.current = greetingId;
          try {
            channel.send(JSON.stringify(assistantGreetingInstruction(greetingId)));
          } catch {
            greetingInstructionIdRef.current = null;
            addDiagnostic("error", "Неуспешно възпроизвеждане на началния поздрав.");
          }
        } else if (speechState === "started") {
          officialNoteRef.current.beginInputTurn();
          commandDetectorRef.current.beginInputTurn();
          speechRecognizedRef.current = false;
          addDiagnostic("system", "Засечена е нова реч; изчаква се разпознаване.");
          registerUserActivity();
        } else if (speechState === "stopped") {
          clearRecognitionTimer();
          if (speechRecognizedRef.current) return;
          recognitionTimerRef.current = window.setTimeout(() => {
            recognitionTimerRef.current = null;
            if (closingRef.current || toolBusyRef.current) return;
            addDiagnostic("system", "Засечена е реч, но няма разбираема транскрипция. Асистентът иска повторение.");
            inactivityTimerRef.current?.feedbackNow();
          }, ASSISTANT_UNRECOGNIZED_SPEECH_MS);
        } else if (event.type === "session.input_transcript.delta" && event.delta) {
          speechRecognizedRef.current = true;
          addTranscriptDelta("heard", event.delta);
          registerUserActivity();
          if (commandDetectorRef.current.observeInputTiming(event.start_ms, event.end_ms)) {
            // GPT-Live exposes timestamps, not a reliable completed-turn event. A clear gap
            // starts only the raw-input accumulator; an active note draft remains intact.
            officialNoteRef.current.beginInputTurn();
          }
          const command = detectAssistantVoiceCommandFromLiveEvent(event, commandDetectorRef.current);
          if (command === "end-session") {
            voiceCloseRef.current();
            return;
          }
          const noteDecision = officialNoteRef.current.pushTranscript(event.delta);
          if (noteDecision === "restart-silence") {
            publishOfficialNotePreview(officialNoteRef.current.currentText());
            armOfficialNoteTimer();
            return;
          }
          if (noteDecision === "confirmed") {
            clearOfficialNoteTimer();
            addDiagnostic("system", "Получено е потвърждение за завършена официална забележка.");
            return;
          }
          if (typeof noteDecision === "object" && noteDecision.kind === "finish") {
            clearOfficialNoteTimer();
            publishOfficialNotePreview(officialNoteRef.current.currentText());
            addDiagnostic("system", "Разпозната е крайна фраза за официалната забележка.");
            sendAidooToolOutput(channel, noteDecision.callId, noteDecision.output);
            updatePhase("listening");
            inactivityTimerRef.current?.start();
            return;
          }
          if (command === "start-dictation") void switchToDictation();
        } else if (event.type === "session.output_transcript.delta" && event.delta) {
          clearRecognitionTimer();
          addTranscriptDelta("assistant", event.delta);
        } else if (event.type === "session.instructions.appended" && event.client_event_id === greetingInstructionIdRef.current) {
          const promptId = `${event.client_event_id}_prompt`;
          greetingInstructionIdRef.current = null;
          try {
            channel.send(JSON.stringify(assistantGreetingPrompt(promptId)));
          } catch {
            addDiagnostic("error", "Неуспешно възпроизвеждане на началния поздрав.");
          }
        } else if (event.type === "session.instructions.appended" && event.client_event_id === clarificationInstructionIdRef.current) {
          const promptId = `${event.client_event_id}_prompt`;
          clarificationInstructionIdRef.current = null;
          if (!clarificationActiveRef.current) return;
          try {
            channel.send(JSON.stringify(assistantClarificationPrompt(promptId)));
          } catch {
            speakClarificationLocally();
          }
        } else if (event.type === "session.instructions.appended" && event.client_event_id === goodbyeInstructionIdRef.current) {
          const promptId = `${event.client_event_id}_prompt`;
          try {
            channel.send(JSON.stringify(assistantGoodbyePrompt(promptId)));
          } catch {
            finishAfterLocalGoodbye();
          }
        } else if (event.type === "response.event") {
          if (!clarificationActiveRef.current) inactivityTimerRef.current?.touch();
          if (event.event?.type === "response.output_text.delta" && event.event.delta) {
            addTranscriptDelta("backend", event.event.delta);
          }
          if (["response.failed", "response.incomplete"].includes(event.event?.type ?? "")) {
            const reason = event.event?.response?.error?.message
              ?? event.event?.response?.incomplete_details?.reason
              ?? `Backend отговорът завърши със статус ${event.event?.type}.`;
            addDiagnostic("error", reason);
            if (!clarificationActiveRef.current) inactivityTimerRef.current?.feedbackNow();
          }
          const backendUsage = backendUsageFromLiveEvent(event);
          if (backendUsage) {
            void invoke("record_live_backend_usage", {
              responseId: backendUsage.responseId,
              model: backendUsage.model,
              usage: backendUsage.usage,
            }).catch(() => undefined);
          }
          let call = functionCallFromLiveEvent(event);
          if (!call?.call_id || handledToolCallsRef.current.has(call.call_id)) return;
          handledToolCallsRef.current.add(call.call_id);
          const noteToolDecision = officialNoteRef.current.intercept(call);
          if (noteToolDecision.kind === "wait") {
            addDiagnostic("system", "Официалната забележка изчаква 10 секунди без продължение.");
            publishOfficialNotePreview(officialNoteRef.current.currentText());
            updatePhase("listening");
            inactivityTimerRef.current?.touch();
            armOfficialNoteTimer();
            return;
          }
          if (noteToolDecision.kind === "ask-again") {
            addDiagnostic("system", "Официалната забележка още не е потвърдена.");
            sendAidooToolOutput(channel, noteToolDecision.callId, noteToolDecision.output);
            updatePhase("listening");
            return;
          }
          if (noteToolDecision.kind === "capture-in-progress") {
            addDiagnostic("system", "Повторното извикване не променя започнатата диктовка на забележката.");
            sendAidooToolOutput(channel, noteToolDecision.callId, noteToolDecision.output);
            updatePhase("listening");
            return;
          }
          call = noteToolDecision.item;
          if (noteToolDecision.kind === "execute") {
            clearOfficialNoteTimer();
            publishOfficialNotePreview(null);
          }
          addDiagnostic("tool-call", describeLiveToolCall(call.name, call.arguments));
          toolBusyRef.current = true;
          inactivityTimerRef.current?.pause();
          updatePhase("working");
          void executeAidooLiveTool(call).then(({ callId, output }) => {
            addDiagnostic("tool-result", describeLiveToolResult(call.name, output));
            if (channel.readyState !== "open" || closingRef.current) return;
            sendAidooToolOutput(channel, callId, output);
            toolBusyRef.current = false;
            updatePhase("listening");
            inactivityTimerRef.current?.start();
          }).catch((reason) => {
            toolBusyRef.current = false;
            fail(reason);
          });
        } else if (event.type === "session.closed") {
          finish("idle");
        } else if (event.type === "error" || event.type === "session.failed") {
          if (goodbyeInstructionIdRef.current) finishAfterLocalGoodbye();
          else fail(event.error?.message ?? "GPT-Live прекъсна разговора.");
        }
      });
      channel.addEventListener("close", () => {
        if (readyRef.current && !closingRef.current) fail("GPT-Live връзката беше прекъсната.");
      });
      peer.addEventListener("connectionstatechange", () => {
        addDiagnostic("system", `WebRTC връзка: ${peer.connectionState}`);
        if (peer.connectionState === "failed" && !closingRef.current) fail("WebRTC връзката с GPT-Live беше прекъсната.");
      });

      await startupTiming.measure("webrtc-offer", async () => {
        const offer = await peer.createOffer();
        await peer.setLocalDescription(offer);
      });
      await startupTiming.measure("ice-gathering", () => waitForIceGathering(peer));
      if (!stillCurrent()) return;
      const sdp = peer.localDescription?.sdp;
      if (!sdp) throw new Error("WebRTC не създаде заявка за разговор.");
      const answer = await startupTiming.measure("session-request", () => withTimeout(
        invoke<LiveSessionAnswer>("create_live_session", { sdp }),
        LIVE_CREATE_TIMEOUT_MS,
        "OpenAI не отговори навреме.",
      ));
      if (!stillCurrent()) return;
      await startupTiming.measure("remote-description", () => peer.setRemoteDescription({ type: "answer", sdp: answer.sdp }));
      if (!readyRef.current) {
        timeoutRef.current = window.setTimeout(() => fail("GPT-Live не потвърди старта на разговора."), SESSION_START_TIMEOUT_MS);
      }
    } catch (reason) {
      if (stillCurrent()) fail(reason);
    }
  }, [addDiagnostic, addTranscriptDelta, armOfficialNoteTimer, clearOfficialNoteTimer, clearRecognitionTimer, clearTimer, diagnosticsEnabled, fail, finish, finishAfterLocalGoodbye, microphoneName, phase, publishOfficialNotePreview, registerRemoteSpeech, registerUserActivity, speakClarificationLocally, switchToDictation, updatePhase]);
  startRef.current = start;

  return { phase, error, diagnostics, start, stop };
}

function monitorRemoteAudio(
  track: MediaStreamTrack,
  audioContextRef: MutableRefObject<AudioContext | null>,
  monitorFrameRef: MutableRefObject<number | null>,
  readyRef: MutableRefObject<boolean>,
  closingRef: MutableRefObject<boolean>,
  toolBusyRef: MutableRefObject<boolean>,
  mountedRef: MutableRefObject<boolean>,
  updatePhase: (phase: LivePhase) => void,
  onRemoteSpeech: () => void,
) {
  try {
    const context = new AudioContext();
    audioContextRef.current = context;
    const source = context.createMediaStreamSource(new MediaStream([track]));
    const analyser = context.createAnalyser();
    analyser.fftSize = 256;
    source.connect(analyser);
    analyser.connect(context.destination);
    void context.resume();
    const samples = new Uint8Array(analyser.fftSize);
    let lastSpeechAt = 0;
    const monitor = () => {
      analyser.getByteTimeDomainData(samples);
      let energy = 0;
      for (const sample of samples) {
        const centered = (sample - 128) / 128;
        energy += centered * centered;
      }
      if (Math.sqrt(energy / samples.length) > 0.025) {
        lastSpeechAt = performance.now();
        onRemoteSpeech();
      }
      if (readyRef.current && !closingRef.current && !toolBusyRef.current && mountedRef.current) {
        updatePhase(performance.now() - lastSpeechAt < 280 ? "speaking" : "listening");
      }
      monitorFrameRef.current = requestAnimationFrame(monitor);
    };
    monitor();
  } catch {
    // The conversation stays usable even if the visual audio meter is unavailable.
  }
}

function waitForIceGathering(peer: RTCPeerConnection) {
  if (peer.iceGatheringState === "complete") return Promise.resolve();
  return new Promise<void>((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      peer.removeEventListener("icegatheringstatechange", onState);
      reject(new Error("WebRTC не успя да подготви мрежовата връзка."));
    }, ICE_GATHERING_TIMEOUT_MS);
    function onState() {
      if (peer.iceGatheringState !== "complete") return;
      window.clearTimeout(timeout);
      peer.removeEventListener("icegatheringstatechange", onState);
      resolve();
    }
    peer.addEventListener("icegatheringstatechange", onState);
    onState();
  });
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timeout = window.setTimeout(() => reject(new Error(message)), timeoutMs);
    promise.then(
      (value) => { window.clearTimeout(timeout); resolve(value); },
      (reason) => { window.clearTimeout(timeout); reject(reason); },
    );
  });
}

function delay(milliseconds: number) {
  return new Promise<void>((resolve) => window.setTimeout(resolve, milliseconds));
}
