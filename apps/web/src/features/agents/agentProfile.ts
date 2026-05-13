import type { SessionSummary } from "../../types";

export function parseAllowedToolsText(value: string): string[] {
  const seen = new Set<string>();
  const tools: string[] = [];
  for (const line of value.split(/\r?\n/)) {
    const tool = line.trim();
    if (!tool || seen.has(tool)) {
      continue;
    }
    seen.add(tool);
    tools.push(tool);
  }
  return tools;
}

export function formatAllowedTools(tools: string[]): string {
  return tools.join("\n");
}

export function sessionsForAgent(sessions: SessionSummary[], agentProfileId: string): SessionSummary[] {
  return sessions
    .filter((session) => session.agent_profile_id === agentProfileId)
    .sort((left, right) => right.updated_at - left.updated_at);
}

export function toggleAllowedTool(allowedTools: string[], toolId: string): string[] {
  if (allowedTools.includes(toolId)) {
    return allowedTools.filter((tool) => tool !== toolId);
  }
  return [...allowedTools, toolId].sort((left, right) => left.localeCompare(right));
}

export function describeAgentProfileChanges(params: {
  currentName: string;
  nextName: string;
  currentWorkspaceRoot: string | null | undefined;
  nextWorkspaceRoot: string;
  currentAllowedTools: string[];
  nextAllowedTools: string[];
}): string[] {
  const changes: string[] = [];
  if (params.nextName.trim() !== params.currentName) {
    changes.push(`name: ${params.currentName} -> ${params.nextName.trim()}`);
  }
  const currentWorkspace = params.currentWorkspaceRoot ?? "";
  const nextWorkspace = params.nextWorkspaceRoot.trim();
  if (nextWorkspace !== currentWorkspace) {
    changes.push(`default_workspace_root: ${currentWorkspace || "<empty>"} -> ${nextWorkspace || "<empty>"}`);
  }
  const currentTools = formatAllowedTools(params.currentAllowedTools);
  const nextTools = formatAllowedTools(params.nextAllowedTools);
  if (nextTools !== currentTools) {
    const added = params.nextAllowedTools.filter((tool) => !params.currentAllowedTools.includes(tool));
    const removed = params.currentAllowedTools.filter((tool) => !params.nextAllowedTools.includes(tool));
    changes.push(`allowed_tools: +${added.length} / -${removed.length}`);
  }
  return changes;
}
