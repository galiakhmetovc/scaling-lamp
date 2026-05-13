import test from "node:test";
import assert from "node:assert/strict";
import { parseToolDebugDetail } from "./toolDetail.ts";

test("parseToolDebugDetail splits arguments and result sections", () => {
  const detail = [
    "Tool Call",
    "id: tool-call-1",
    "tool: web_fetch",
    "status: completed",
    "summary: web_fetch url=https://example.com",
    "",
    "arguments:",
    "{",
    "  \"url\": \"https://example.com\"",
    "}",
    "",
    "result:",
    "result_summary: web_fetch status=200",
    "result_byte_len: 42",
    "result_truncated: false",
    "result_artifact_id: <none>",
    "",
    "result_preview:",
    "{",
    "  \"body\": \"ok\"",
    "}"
  ].join("\n");

  const parsed = parseToolDebugDetail(detail);

  assert.equal(parsed.meta.get("id"), "tool-call-1");
  assert.equal(parsed.meta.get("tool"), "web_fetch");
  assert.equal(parsed.arguments?.includes("\"url\""), true);
  assert.equal(parsed.result.get("result_summary"), "web_fetch status=200");
  assert.equal(parsed.resultPreview?.includes("\"body\""), true);
});
