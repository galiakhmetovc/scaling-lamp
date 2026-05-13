export type ToolLikeCall = {
  id: string;
  run_id: string;
  tool_name: string;
  status: string;
  error?: string | null;
};

export type ChatLikeLine = {
  role: string;
  content: string;
  tool_name?: string | null;
};

export type ToolStats = {
  total: number;
  succeeded: number;
  failed: number;
  mcpTotal: number;
  mcpSucceeded: number;
  mcpFailed: number;
};

export function isMcpToolCall(tool: Pick<ToolLikeCall, "tool_name">): boolean {
  return tool.tool_name.startsWith("mcp_") || tool.tool_name.startsWith("mcp__");
}

function isSuccessfulToolCall(tool: Pick<ToolLikeCall, "status" | "error">): boolean {
  return tool.status === "completed" && !tool.error;
}

export function buildToolStats(tools: ToolLikeCall[], runId?: string | null): ToolStats {
  const scopedTools = runId ? tools.filter((tool) => tool.run_id === runId) : tools;
  const mcpTools = scopedTools.filter(isMcpToolCall);
  const succeeded = scopedTools.filter(isSuccessfulToolCall).length;
  const mcpSucceeded = mcpTools.filter(isSuccessfulToolCall).length;

  return {
    total: scopedTools.length,
    succeeded,
    failed: scopedTools.length - succeeded,
    mcpTotal: mcpTools.length,
    mcpSucceeded,
    mcpFailed: mcpTools.length - mcpSucceeded
  };
}

export function isLowSignalChatLine(line: ChatLikeLine): boolean {
  if (line.tool_name) {
    return true;
  }
  if (line.role !== "system") {
    return false;
  }
  const content = line.content.trim().toLowerCase();
  return /^process\s+\S+\s+(completed|waiting)\b/.test(content);
}
