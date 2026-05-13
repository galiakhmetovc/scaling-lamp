import test from "node:test";
import assert from "node:assert/strict";
import { formatAllowedTools, parseAllowedToolsText, sessionsForAgent } from "./agentProfile.ts";
import type { SessionSummary } from "../../types.ts";

function session(id: string, agentProfileId: string, updatedAt: number): SessionSummary {
  return {
    id,
    title: id,
    agent_profile_id: agentProfileId,
    agent_name: agentProfileId,
    reasoning_visible: false,
    compactifications: 0,
    auto_approve: false,
    context_tokens: 0,
    has_pending_approval: false,
    message_count: 0,
    background_job_count: 0,
    running_background_job_count: 0,
    queued_background_job_count: 0,
    created_at: updatedAt,
    updated_at: updatedAt
  };
}

test("parseAllowedToolsText trims blank lines and deduplicates tools in display order", () => {
  assert.deepEqual(parseAllowedToolsText(" fs_read_text\n\nweb_search\nfs_read_text\n mcp "), [
    "fs_read_text",
    "web_search",
    "mcp"
  ]);
});

test("formatAllowedTools renders one tool per line", () => {
  assert.equal(formatAllowedTools(["fs_read_text", "web_search"]), "fs_read_text\nweb_search");
});

test("sessionsForAgent returns matching sessions newest first", () => {
  const sessions = [session("old", "default", 10), session("other", "judge", 30), session("new", "default", 20)];

  assert.deepEqual(
    sessionsForAgent(sessions, "default").map((item) => item.id),
    ["new", "old"]
  );
});
