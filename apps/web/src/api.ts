import type {
  AgentDetail,
  AgentFile,
  AgentFiles,
  AgentFileWriteResult,
  AgentSchedule,
  AgentScheduleCreateOptions,
  AgentScheduleUpdatePatch,
  AgentUpdatePatch,
  ArtifactFile,
  ArtifactFileSummary,
  DeliveryTarget,
  DeliveryTargetCreateOptions,
  DeliveryTargetUpdatePatch,
  KvList,
  MemoryRecallPreview,
  McpConnector,
  McpPromptGet,
  McpPromptList,
  McpResourceList,
  McpResourceRead,
  PendingApproval,
  SemanticMemoryList,
  SemanticMemorySearch,
  SessionDebug,
  SessionOutputRoute,
  SessionOutputRouteCreateOptions,
  SessionOutputRouteUpdatePatch,
  SessionPreferencesPatch,
  SessionSkillStatus,
  SessionSummary,
  SessionTask,
  SessionTranscript,
  ToolCatalog,
  WebSnapshot,
  WorkerOutcome,
  WorkspaceFile,
  WorkspaceList,
  WorkspaceMkdirResult,
  WorkspaceTrashResult,
  WorkspaceWriteResult
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
  eventsUrl() {
    return "/api/events";
  },
  snapshot(signal?: AbortSignal) {
    return requestJson<WebSnapshot>(endpoint("/v1/web/snapshot"), { signal });
  },
  toolCatalog(signal?: AbortSignal) {
    return requestJson<ToolCatalog>(endpoint("/v1/tools/catalog"), { signal });
  },
  mcpConnectors(signal?: AbortSignal) {
    return requestJson<McpConnector[]>(endpoint("/v1/mcp/connectors"), { signal });
  },
  deliveryTargets(signal?: AbortSignal) {
    return requestJson<unknown>(endpoint("/v1/delivery-targets"), { signal });
  },
  createDeliveryTarget(targetId: string, options: DeliveryTargetCreateOptions) {
    return requestJson<unknown>(endpoint("/v1/delivery-targets"), {
      method: "POST",
      body: JSON.stringify({ target_id: targetId, options })
    });
  },
  updateDeliveryTarget(targetId: string, patch: DeliveryTargetUpdatePatch) {
    return requestJson<unknown>(endpoint(`/v1/delivery-targets/${encodeURIComponent(targetId)}`), {
      method: "PATCH",
      body: JSON.stringify({ patch })
    });
  },
  sessionOutputRoutes(signal?: AbortSignal) {
    return requestJson<unknown>(endpoint("/v1/session-output-routes"), { signal });
  },
  createSessionOutputRoute(sessionId: string, targetId: string, options: SessionOutputRouteCreateOptions) {
    return requestJson<unknown>(endpoint("/v1/session-output-routes"), {
      method: "POST",
      body: JSON.stringify({ session_id: sessionId, target_id: targetId, options })
    });
  },
  updateSessionOutputRoute(routeId: string, patch: SessionOutputRouteUpdatePatch) {
    return requestJson<unknown>(endpoint(`/v1/session-output-routes/${encodeURIComponent(routeId)}`), {
      method: "PATCH",
      body: JSON.stringify({ patch })
    });
  },
  createMcpConnector(
    id: string,
    options: Pick<McpConnector, "transport" | "command" | "args" | "env" | "cwd" | "enabled">
  ) {
    return requestJson<{ connector: McpConnector }>(endpoint("/v1/mcp/connectors"), {
      method: "POST",
      body: JSON.stringify({ id, options })
    });
  },
  mcpResources(options: { connectorId?: string; query?: string; limit?: number; offset?: number } = {}, signal?: AbortSignal) {
    return requestJson<McpResourceList>(
      endpoint(
        `/v1/mcp/resources${queryString({
          connector_id: options.connectorId ?? "",
          query: options.query ?? "",
          limit: options.limit ?? 50,
          offset: options.offset ?? 0
        })}`
      ),
      { signal }
    );
  },
  mcpReadResource(connectorId: string, uri: string) {
    return requestJson<McpResourceRead>(endpoint("/v1/mcp/resources/read"), {
      method: "POST",
      body: JSON.stringify({ connector_id: connectorId, uri })
    });
  },
  mcpPrompts(options: { connectorId?: string; query?: string; limit?: number; offset?: number } = {}, signal?: AbortSignal) {
    return requestJson<McpPromptList>(
      endpoint(
        `/v1/mcp/prompts${queryString({
          connector_id: options.connectorId ?? "",
          query: options.query ?? "",
          limit: options.limit ?? 50,
          offset: options.offset ?? 0
        })}`
      ),
      { signal }
    );
  },
  mcpGetPrompt(connectorId: string, name: string, args?: Record<string, string>) {
    return requestJson<McpPromptGet>(endpoint("/v1/mcp/prompts/get"), {
      method: "POST",
      body: JSON.stringify({ connector_id: connectorId, name, arguments: args ?? null })
    });
  },
  semanticMemoryList(
    sessionId: string,
    options: { scope?: string; limit?: number; offset?: number } = {},
    signal?: AbortSignal
  ) {
    return requestJson<SemanticMemoryList>(
      endpoint(
        `/v1/memory/semantic${queryString({
          session_id: sessionId,
          scope: options.scope ?? "workspace",
          limit: options.limit ?? 20,
          offset: options.offset ?? 0
        })}`
      ),
      { signal }
    );
  },
  semanticMemorySearch(
    sessionId: string,
    options: { query: string; scope?: string; limit?: number; filters?: unknown },
    signal?: AbortSignal
  ) {
    return requestJson<SemanticMemorySearch>(endpoint("/v1/memory/semantic/search"), {
      method: "POST",
      body: JSON.stringify({
        session_id: sessionId,
        query: options.query,
        scope: options.scope ?? "workspace",
        limit: options.limit ?? 20,
        filters: options.filters ?? null
      }),
      signal
    });
  },
  semanticMemoryUpdate(memoryId: string, text: string, metadata: unknown = null) {
    return requestJson<{ memory_id: string; updated: boolean }>(
      endpoint(`/v1/memory/semantic/${encodeURIComponent(memoryId)}`),
      {
        method: "PATCH",
        body: JSON.stringify({ text, metadata })
      }
    );
  },
  semanticMemoryDelete(memoryId: string) {
    return requestJson<{ memory_id: string; deleted: boolean }>(
      endpoint(`/v1/memory/semantic/${encodeURIComponent(memoryId)}`),
      { method: "DELETE" }
    );
  },
  kvList(
    sessionId: string,
    options: { scope?: string; prefix?: string; limit?: number; offset?: number } = {},
    signal?: AbortSignal
  ) {
    return requestJson<KvList>(
      endpoint(
        `/v1/kv${queryString({
          session_id: sessionId,
          scope: options.scope ?? "workspace",
          prefix: options.prefix ?? "",
          limit: options.limit ?? 50,
          offset: options.offset ?? 0
        })}`
      ),
      { signal }
    );
  },
  kvPut(
    sessionId: string,
    input: {
      key: string;
      value: unknown;
      scope?: string;
      metadata?: unknown;
      expected_revision?: number | null;
      ttl_seconds?: number | null;
    }
  ) {
    return requestJson<{ entry: unknown }>(endpoint("/v1/kv"), {
      method: "PUT",
      body: JSON.stringify({
        session_id: sessionId,
        key: input.key,
        value: input.value,
        scope: input.scope ?? "workspace",
        metadata: input.metadata ?? null,
        expected_revision: input.expected_revision ?? null,
        ttl_seconds: input.ttl_seconds ?? null
      })
    });
  },
  kvDelete(sessionId: string, input: { key: string; scope?: string; expected_revision?: number | null }) {
    return requestJson<{ key: string; deleted: boolean }>(endpoint("/v1/kv"), {
      method: "DELETE",
      body: JSON.stringify({
        session_id: sessionId,
        key: input.key,
        scope: input.scope ?? "workspace",
        expected_revision: input.expected_revision ?? null
      })
    });
  },
  memoryRecallPreview(sessionId: string, query?: string, signal?: AbortSignal) {
    return requestJson<MemoryRecallPreview>(endpoint("/v1/memory/recall-preview"), {
      method: "POST",
      body: JSON.stringify({
        session_id: sessionId,
        query: query || null
      }),
      signal
    });
  },
  updateMcpConnector(
    connectorId: string,
    patch: Partial<Pick<McpConnector, "command" | "args" | "env" | "cwd" | "enabled">>
  ) {
    return requestJson<{ connector: McpConnector }>(endpoint(`/v1/mcp/connectors/${encodeURIComponent(connectorId)}`), {
      method: "PATCH",
      body: JSON.stringify({ patch })
    });
  },
  restartMcpConnector(connectorId: string) {
    return requestJson<{ connector: McpConnector }>(
      endpoint(`/v1/mcp/connectors/${encodeURIComponent(connectorId)}/restart`),
      {
        method: "POST",
        body: JSON.stringify({})
      }
    );
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
  sessionsForAgent(agentProfileId: string, limit?: number, offset?: number, signal?: AbortSignal) {
    const params = new URLSearchParams({ agent_profile_id: agentProfileId });
    if (limit) {
      params.set("limit", String(limit));
    }
    if (offset) {
      params.set("offset", String(offset));
    }
    return requestJson<SessionSummary[]>(endpoint(`/v1/sessions?${params.toString()}`), { signal });
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
  agentDetail(agentId: string, signal?: AbortSignal) {
    return requestJson<AgentDetail>(endpoint(`/v1/agents/${encodeURIComponent(agentId)}`), { signal });
  },
  updateAgent(agentId: string, patch: AgentUpdatePatch) {
    return requestJson<AgentDetail>(endpoint(`/v1/agents/${encodeURIComponent(agentId)}`), {
      method: "PATCH",
      body: JSON.stringify(patch)
    });
  },
  deleteAgent(agentId: string) {
    return requestJson<{ deleted: boolean }>(endpoint(`/v1/agents/${encodeURIComponent(agentId)}`), {
      method: "DELETE"
    });
  },
  agentSchedules(signal?: AbortSignal) {
    return requestJson<{ schedules: AgentSchedule[] }>(endpoint("/v1/agent-schedules/list"), { signal });
  },
  createAgentSchedule(id: string, options: AgentScheduleCreateOptions) {
    return requestJson<{ message: string }>(endpoint("/v1/agent-schedules"), {
      method: "POST",
      body: JSON.stringify({ id, options })
    });
  },
  updateAgentSchedule(id: string, patch: AgentScheduleUpdatePatch) {
    return requestJson<{ message: string }>(endpoint(`/v1/agent-schedules/${encodeURIComponent(id)}`), {
      method: "PATCH",
      body: JSON.stringify({ patch })
    });
  },
  runAgentScheduleNow(id: string) {
    return requestJson<{ schedule: AgentSchedule }>(endpoint(`/v1/agent-schedules/${encodeURIComponent(id)}/run-now`), {
      method: "POST",
      body: JSON.stringify({})
    });
  },
  deleteAgentSchedule(id: string) {
    return requestJson<{ message: string }>(endpoint(`/v1/agent-schedules/${encodeURIComponent(id)}`), {
      method: "DELETE"
    });
  },
  agentFiles(agentId: string, signal?: AbortSignal) {
    return requestJson<AgentFiles>(endpoint(`/v1/agents/${encodeURIComponent(agentId)}/files`), { signal });
  },
  agentFileRead(agentId: string, path: string, signal?: AbortSignal) {
    return requestJson<AgentFile>(
      endpoint(`/v1/agents/${encodeURIComponent(agentId)}/files/read${queryString({ path })}`),
      { signal }
    );
  },
  agentFileWrite(agentId: string, path: string, content: string, mode: "create" | "overwrite" | "upsert") {
    return requestJson<AgentFileWriteResult>(endpoint(`/v1/agents/${encodeURIComponent(agentId)}/files/write`), {
      method: "POST",
      body: JSON.stringify({ path, content, mode })
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
  enableSessionSkill(sessionId: string, name: string) {
    return requestJson<SessionSkillStatus[]>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/skills/enable`), {
      method: "POST",
      body: JSON.stringify({ name })
    });
  },
  disableSessionSkill(sessionId: string, name: string) {
    return requestJson<SessionSkillStatus[]>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/skills/disable`), {
      method: "POST",
      body: JSON.stringify({ name })
    });
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
  workspaceWrite(sessionId: string, path: string, content: string, mode: "create" | "overwrite" | "upsert") {
    return requestJson<WorkspaceWriteResult>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/workspace/write`), {
      method: "POST",
      body: JSON.stringify({ path, content, mode })
    });
  },
  workspaceUpload(sessionId: string, path: string, file: File, mode: "create" | "overwrite" | "upsert") {
    return requestJson<WorkspaceWriteResult>(
      endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/workspace/upload${queryString({ path, mode })}`),
      {
        method: "POST",
        headers: {
          "content-type": file.type || "application/octet-stream"
        },
        body: file
      }
    );
  },
  workspaceMkdir(sessionId: string, path: string) {
    return requestJson<WorkspaceMkdirResult>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/workspace/mkdir`), {
      method: "POST",
      body: JSON.stringify({ path })
    });
  },
  workspaceTrash(sessionId: string, path: string) {
    return requestJson<WorkspaceTrashResult>(endpoint(`/v1/sessions/${encodeURIComponent(sessionId)}/workspace/trash`), {
      method: "POST",
      body: JSON.stringify({ path })
    });
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
