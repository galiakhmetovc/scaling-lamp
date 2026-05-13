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
