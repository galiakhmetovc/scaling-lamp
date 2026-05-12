import type {
  SessionDebug,
  SessionSummary,
  SessionTask,
  SessionTranscript,
  WebSnapshot,
  WorkerOutcome
} from "./types";

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(path, {
    ...init,
    headers: {
      accept: "application/json",
      ...(init?.body ? { "content-type": "application/json" } : {}),
      ...init?.headers
    }
  });
  const text = await response.text();
  const payload = text ? JSON.parse(text) : null;
  if (!response.ok) {
    const message = payload?.error ?? payload?.detail ?? response.statusText;
    throw new Error(`${response.status} ${message}`);
  }
  return payload as T;
}

function endpoint(path: string): string {
  return `/api/agentd${path}`;
}

export const api = {
  snapshot(signal?: AbortSignal) {
    return requestJson<WebSnapshot>(endpoint("/v1/web/snapshot"), { signal });
  },
  sessions(limit?: number, offset?: number, signal?: AbortSignal) {
    const params = new URLSearchParams();
    if (limit) {
      params.set("limit", String(limit));
    }
    if (offset) {
      params.set("offset", String(offset));
    }
    const query = params.size > 0 ? `?${params.toString()}` : "";
    return requestJson<SessionSummary[]>(endpoint(`/v1/sessions${query}`), { signal });
  },
  createSession(title: string, agentIdentifier?: string) {
    return requestJson<SessionSummary>(endpoint("/v1/sessions"), {
      method: "POST",
      body: JSON.stringify({
        title,
        agent_identifier: agentIdentifier || null
      })
    });
  },
  createAgent(name: string, templateIdentifier?: string) {
    return requestJson<{ message: string }>(endpoint("/v1/agents"), {
      method: "POST",
      body: JSON.stringify({
        name,
        template_identifier: templateIdentifier || null
      })
    });
  },
  transcript(sessionId: string, limit = 160, signal?: AbortSignal) {
    return requestJson<SessionTranscript>(
      endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/transcript-tail/${limit}`),
      { signal }
    );
  },
  debug(sessionId: string, signal?: AbortSignal) {
    return requestJson<SessionDebug>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/debug`), {
      signal
    });
  },
  tasks(sessionId: string, signal?: AbortSignal) {
    return requestJson<SessionTask[]>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/tasks`), {
      signal
    });
  },
  run(sessionId: string, signal?: AbortSignal) {
    return requestJson<unknown>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/run`), { signal });
  },
  sendMessage(sessionId: string, message: string) {
    return requestJson<WorkerOutcome>(endpoint("/v1/chat/turn"), {
      method: "POST",
      body: JSON.stringify({
        session_id: sessionId,
        message,
        now: Math.floor(Date.now() / 1000),
        surface: "web",
        entrypoint: "teamd.web_console"
      })
    });
  },
  cancelRun(sessionId: string) {
    return requestJson<unknown>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/cancel-run`), {
      method: "POST"
    });
  },
  cancelAllWork(sessionId: string) {
    return requestJson<unknown>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/cancel-all-work`), {
      method: "POST"
    });
  },
  deleteSession(sessionId: string) {
    return requestJson<{ deleted: boolean }>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}`), {
      method: "DELETE"
    });
  }
};
