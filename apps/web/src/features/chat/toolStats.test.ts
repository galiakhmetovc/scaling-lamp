import test from "node:test";
import assert from "node:assert/strict";
import { buildToolStats, isLowSignalChatLine, isMcpToolCall, type ChatLikeLine, type ToolLikeCall } from "./toolStats.ts";

function tool(overrides: Partial<ToolLikeCall>): ToolLikeCall {
  return {
    id: overrides.id ?? "tool-1",
    run_id: overrides.run_id ?? "run-1",
    tool_name: overrides.tool_name ?? "fs_read_text",
    status: overrides.status ?? "completed",
    error: overrides.error ?? null
  };
}

function line(overrides: Partial<ChatLikeLine>): ChatLikeLine {
  return {
    role: overrides.role ?? "system",
    content: overrides.content ?? "",
    tool_name: overrides.tool_name ?? null
  };
}

test("buildToolStats counts regular and MCP calls for one run", () => {
  const stats = buildToolStats(
    [
      tool({ id: "a", run_id: "run-1", tool_name: "fs_read_text" }),
      tool({ id: "b", run_id: "run-1", tool_name: "mcp__silverbullet__read_note" }),
      tool({ id: "c", run_id: "run-1", tool_name: "mcp_search_resources", status: "failed", error: "boom" }),
      tool({ id: "d", run_id: "run-2", tool_name: "web_search" })
    ],
    "run-1"
  );

  assert.deepEqual(stats, {
    total: 3,
    succeeded: 2,
    failed: 1,
    mcpTotal: 2,
    mcpSucceeded: 1,
    mcpFailed: 1
  });
});

test("isMcpToolCall recognizes dynamic and built-in MCP tool names", () => {
  assert.equal(isMcpToolCall(tool({ tool_name: "mcp__silverbullet__read_note" })), true);
  assert.equal(isMcpToolCall(tool({ tool_name: "mcp_search_resources" })), true);
  assert.equal(isMcpToolCall(tool({ tool_name: "memory_search" })), false);
});

test("isLowSignalChatLine hides synthetic process status but keeps real system errors", () => {
  assert.equal(isLowSignalChatLine(line({ content: "process exec-1 completed with Some(0)" })), true);
  assert.equal(isLowSignalChatLine(line({ content: "process exec-2 waiting for output" })), true);
  assert.equal(isLowSignalChatLine(line({ content: "chat failed: provider response did not include assistant text" })), false);
  assert.equal(isLowSignalChatLine(line({ role: "assistant", content: "process exec-1 completed with Some(0)" })), false);
});
