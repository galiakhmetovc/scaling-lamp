import type {
  ArtifactFile,
  ArtifactFileSummary,
  PendingApproval,
  SessionDebug,
  SessionPreferencesPatch,
  SessionSkillStatus,
  SessionSummary,
  SessionTask,
  SessionTranscript,
  WebSnapshot,
  WorkerOutcome,
  WorkspaceFile,
  WorkspaceList
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

function queryString(params: Record<string, string | number | boolean | null | undefined>): string {
  const query = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null && value !== "") {
      query.set(key, String(value));
    }
  }
  const rendered = query.toString();
  return rendered ? `?${rendered}` : "";
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
  pendingApprovals(sessionId: string, signal?: AbortSignal) {
    return requestJson<PendingApproval[]>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/approvals`), { signal });
  },
  sessionPlan(sessionId: string, signal?: AbortSignal) {
    return requestJson<{ plan: string }>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/plan`), { signal });
  },
  sessionSkills(sessionId: string, signal?: AbortSignal) {
    return requestJson<SessionSkillStatus[]>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/skills`), { signal });
  },
  workspaceList(
    sessionId: string,
    options: { path?: string; recursive?: boolean; limit?: number; offset?: number } = {},
    signal?: AbortSignal
  ) {
    return requestJson<WorkspaceList>(
      endpoint(
        `/v1/sessions/${encodeURIComponent(sessionId)}/workspace/list${queryString({
          path: options.path ?? "",
          recursive: options.recursive ?? false,
          limit: options.limit ?? 100,
          offset: options.offset ?? 0
        })}`
      ),
      { signal }
    );
  },
  workspaceRead(sessionId: string, path: string, signal?: AbortSignal) {
    return requestJson<WorkspaceFile>(
      endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/workspace/read${queryString({ path })}`),
      { signal }
    );
  },
  workspaceDownloadUrl(sessionId: string, path: string) {
    return endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/workspace/download${queryString({ path })}`);
  },
  artifactFiles(sessionId: string, signal?: AbortSignal) {
    return requestJson<{ artifacts: ArtifactFileSummary[] }>(
      endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/artifact-files`),
      { signal }
    );
  },
  artifactFile(sessionId: string, artifactId: string, signal?: AbortSignal) {
    return requestJson<ArtifactFile>(
      endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/artifact-files/${encodeURIComponent(artifactId)}`),
      { signal }
    );
  },
  artifactDownloadUrl(sessionId: string, artifactId: string) {
    return endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/artifact-files/${encodeURIComponent(artifactId)}/download`);
  },
  updateSessionPreferences(sessionId: string, patch: SessionPreferencesPatch) {
    return requestJson<SessionSummary>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/preferences`), {
      method: "PATCH",
      body: JSON.stringify(patch)
    });
  },
  compactSession(sessionId: string) {
    return requestJson<SessionSummary>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/compact`), {
      method: "POST",
      body: JSON.stringify({})
    });
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
  approveRun(runId: string, approvalId: string) {
    return requestJson<WorkerOutcome>(endpoint("/v1/runs/approve"), {
      method: "POST",
      body: JSON.stringify({
        run_id: runId,
        approval_id: approvalId,
        now: Math.floor(Date.now() / 1000)
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
