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

export type TimelineMarkdownBlock = {
  summary: string;
  body: string;
  collapsible: boolean;
};

export type LiveToolView = {
  summary: string;
  body: string;
  collapsible: boolean;
  state: "running" | "done" | "error";
};

export function emptySessionUIState(): SessionUIState {
  return { streaming: "", status: "idle", toolLog: [], btwRuns: [] };
}

export function approximateContextTokens(session: SessionSnapshot | null, input: string): number {
  if (!session) {
    return 0;
  }
  let tokens = session.base_context_tokens;
  tokens += Math.max(0, Math.ceil(input.length / 4));
  for (const draft of session.queued_drafts) {
    tokens += Math.max(0, Math.ceil(draft.text.length / 4));
  }
  return Math.max(1, tokens);
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

export function buildTimelineMarkdownBlock(kind: "message" | "tool" | "plan", content: string): TimelineMarkdownBlock {
  const trimmed = content.trim();
  if (kind !== "tool") {
    return { summary: trimmed, body: "", collapsible: false };
  }
  const parts = trimmed.split(/\n\s*\n/, 2);
  const summary = parts[0]?.trim() ?? trimmed;
  const body = parts[1]?.trim() ?? "";
  return {
    summary,
    body,
    collapsible: body.length > 0,
  };
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
      if (event.status === "running") {
        next.toolLog = [];
      }
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

export function mergeSessionHistory(current: SessionSnapshot | undefined, incoming: SessionSnapshot): SessionSnapshot {
  if (!current || current.session_id !== incoming.session_id) {
    return incoming;
  }
  const preservedCount = Math.max(0, incoming.history.total_count - incoming.history.loaded_count);
  const olderPrefix = current.timeline.slice(0, Math.min(current.timeline.length, preservedCount));
  return {
    ...incoming,
    timeline: [...olderPrefix, ...incoming.timeline],
    history: {
      ...incoming.history,
      loaded_count: olderPrefix.length + incoming.timeline.length,
      has_more: olderPrefix.length+incoming.timeline.length < incoming.history.total_count,
    },
  };
}

export function prependOlderTimeline(
  session: SessionSnapshot,
  payload: {
    timeline: SessionSnapshot["timeline"];
    loaded_count: number;
    total_count: number;
    has_more: boolean;
    window_limit: number;
  },
): SessionSnapshot {
  return {
    ...session,
    timeline: [...payload.timeline, ...session.timeline],
    history: {
      loaded_count: payload.loaded_count,
      total_count: payload.total_count,
      has_more: payload.has_more,
      window_limit: payload.window_limit,
    },
  };
}

export function buildLiveToolView(tool: NonNullable<UIEvent["tool"]>): LiveToolView {
  const name = tool.name || "tool";
  if (tool.phase === "started") {
    return {
      summary: `Tool started: \`${name}\``,
      body: tool.arguments && Object.keys(tool.arguments).length > 0 ? summarizeObject(tool.arguments) : "",
      collapsible: Boolean(tool.arguments && Object.keys(tool.arguments).length > 0),
      state: "running",
    };
  }
  if (tool.error_text) {
    return {
      summary: tool.error_text.includes("requires approval") ? `Approval required: \`${name}\`` : `Tool failed: \`${name}\``,
      body: summarizeLines(tool.error_text, tool.result_text),
      collapsible: summarizeLines(tool.error_text, tool.result_text).length > 0,
      state: "error",
    };
  }
  return {
    summary: `Tool done: \`${name}\``,
    body: summarizeLines(tool.result_text),
    collapsible: summarizeLines(tool.result_text).length > 0,
    state: "done",
  };
}

function summarizeObject(value: Record<string, unknown>): string {
  const text = JSON.stringify(value, null, 2);
  return text.length > 240 ? `${text.slice(0, 237)}...` : text;
}

function summarizeLines(...parts: Array<string | undefined>): string {
  const text = parts
    .map((part) => (part ?? "").trim())
    .filter(Boolean)
    .join("\n\n");
  if (text.length <= 240) {
    return text;
  }
  return `${text.slice(0, 237)}...`;
}
