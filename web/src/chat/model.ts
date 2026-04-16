import type { ProviderResultPayload, SessionSnapshot, UIEvent } from "../lib/types";

export type BtwRun = {
  id: string;
  prompt: string;
  active: boolean;
  error?: string;
  result?: ProviderResultPayload;
};

export type SessionUIState = {
  streaming: string;
  status: string;
  toolLog: NonNullable<UIEvent["tool"]>[];
  btwRuns: BtwRun[];
  lastResult?: ProviderResultPayload;
};

export type ChatStatusView = {
  provider: string;
  model: string;
  runText: string;
  contextTokens: number;
  queueCount: number;
  activeBtwCount: number;
  statusText: string;
  lastUsageText: string;
};

export type BtwRunView = {
  id: string;
  prompt: string;
  statusText: string;
  body: string;
  providerMeta?: string;
};

export function emptySessionUIState(): SessionUIState {
  return { streaming: "", status: "idle", toolLog: [], btwRuns: [] };
}

export function approximateContextTokens(session: SessionSnapshot | null, input: string): number {
  if (!session) {
    return 0;
  }
  let chars = 0;
  for (const message of session.transcript) {
    chars += message.content.length;
  }
  chars += input.length;
  for (const draft of session.queued_drafts) {
    chars += draft.text.length;
  }
  return Math.max(1, Math.ceil(chars / 4));
}

export function buildChatStatus(input: {
  session: SessionSnapshot | null;
  input: string;
  now: Date;
  activeBtwCount: number;
  uiStatus?: string;
}): ChatStatusView {
  const { session, input: draftInput, now, activeBtwCount, uiStatus } = input;
  const provider = session?.main_run.provider || "provider";
  const model = session?.main_run.model || "model";
  const contextTokens = approximateContextTokens(session, draftInput);
  const running = Boolean(session?.main_run.active);
  const startedAt = session?.main_run.started_at ? new Date(session.main_run.started_at) : null;
  const elapsedMs = running && startedAt ? Math.max(0, now.getTime() - startedAt.getTime()) : 0;
  const totalSeconds = Math.floor(elapsedMs / 1000);
  const runText = running ? `running ${String(Math.floor(totalSeconds / 60)).padStart(2, "0")}:${String(totalSeconds % 60).padStart(2, "0")}` : "idle";
  const totalTokens = session?.main_run.total_tokens ?? 0;

  return {
    provider,
    model,
    runText,
    contextTokens,
    queueCount: session?.queued_drafts.length ?? 0,
    activeBtwCount,
    statusText: uiStatus || "ready",
    lastUsageText: totalTokens > 0 ? `last=${totalTokens}` : "",
  };
}

export function buildBtwRuns(runs: BtwRun[]): BtwRunView[] {
  return runs.map((run) => {
    if (run.active) {
      return { id: run.id, prompt: run.prompt, statusText: "running", body: "_Waiting for response..._" };
    }
    if (run.error) {
      return { id: run.id, prompt: run.prompt, statusText: "failed", body: `Error: ${run.error}` };
    }
    const result = run.result;
    const providerMeta = result ? `${result.provider} | ${result.model} | ${result.total_tokens} tok` : undefined;
    return {
      id: run.id,
      prompt: run.prompt,
      statusText: "done",
      body: result?.content ?? "",
      providerMeta,
    };
  });
}

export function applySessionUIEvent(current: SessionUIState | undefined, event: UIEvent): SessionUIState {
  const next = { ...emptySessionUIState(), ...current };
  switch (event.kind) {
    case "stream.text":
      next.streaming += event.text || "";
      break;
    case "tool.started":
    case "tool.completed":
      if (event.tool) {
        next.toolLog = [...next.toolLog, event.tool].slice(-200);
      }
      break;
    case "status.changed":
      next.status = event.status || next.status;
      break;
    case "run.completed":
      next.status = "done";
      next.streaming = "";
      break;
  }
  return next;
}

export function markMainRunStarted(session: SessionSnapshot, startedAt: Date): SessionSnapshot {
  return {
    ...session,
    main_run_active: true,
    main_run: {
      ...session.main_run,
      active: true,
      started_at: startedAt.toISOString(),
      input_tokens: 0,
      output_tokens: 0,
      total_tokens: 0,
    },
  };
}

export function storeMainRunResult(current: SessionUIState | undefined, result: ProviderResultPayload): SessionUIState {
  return {
    ...emptySessionUIState(),
    ...current,
    streaming: "",
    status: "idle",
    lastResult: result,
  };
}

export function syncMainRunFromUIEvent(session: SessionSnapshot | null, event: UIEvent, now: Date): SessionSnapshot | null {
  if (!session || event.session_id !== session.session_id) {
    return session;
  }
  if (event.kind === "status.changed" && event.status === "running" && !session.main_run.active) {
    return markMainRunStarted(session, now);
  }
  return session;
}

export function appendBtwRun(current: SessionUIState | undefined, run: BtwRun): SessionUIState {
  const next = { ...emptySessionUIState(), ...current };
  return { ...next, btwRuns: [...next.btwRuns, run] };
}

export function resolveBtwRun(current: SessionUIState | undefined, runID: string, patch: Partial<BtwRun>): SessionUIState {
  const next = { ...emptySessionUIState(), ...current };
  return {
    ...next,
    btwRuns: next.btwRuns.map((run) => (run.id === runID ? { ...run, ...patch } : run)),
  };
}
