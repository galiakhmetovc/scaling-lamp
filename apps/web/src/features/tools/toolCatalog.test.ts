import assert from "node:assert/strict";
import test from "node:test";
import type { ToolCatalogItem } from "../../types.ts";
import { agentAllowsTool, groupToolCatalogByFamily, summarizeToolCatalog } from "./toolCatalog.ts";

function item(overrides: Partial<ToolCatalogItem>): ToolCatalogItem {
  return {
    id: overrides.id ?? "fs_read_text",
    family: overrides.family ?? "fs",
    origin: overrides.origin ?? "built_in",
    connector_id: overrides.connector_id ?? null,
    remote_name: overrides.remote_name ?? null,
    title: overrides.title ?? null,
    description: overrides.description ?? "Read a file",
    read_only: overrides.read_only ?? true,
    destructive: overrides.destructive ?? false,
    requires_approval: overrides.requires_approval ?? false,
    automatic: overrides.automatic ?? true,
    available: overrides.available ?? true,
    availability_note: overrides.availability_note ?? null,
    input_schema: overrides.input_schema ?? { type: "object" }
  };
}

test("groupToolCatalogByFamily orders known families and sorts tools by id", () => {
  const groups = groupToolCatalogByFamily([
    item({ id: "mcp__silverbullet__read_note", family: "mcp", origin: "mcp", connector_id: "silverbullet" }),
    item({ id: "web_search", family: "web" }),
    item({ id: "fs_read_text", family: "fs" }),
    item({ id: "fs_glob", family: "fs" })
  ]);

  assert.deepEqual(
    groups.map((group) => [group.family, group.tools.map((tool) => tool.id)]),
    [
      ["fs", ["fs_glob", "fs_read_text"]],
      ["web", ["web_search"]],
      ["mcp", ["mcp__silverbullet__read_note"]]
    ]
  );
});

test("summarizeToolCatalog counts built-in, MCP, destructive, and unavailable tools", () => {
  assert.deepEqual(
    summarizeToolCatalog([
      item({ id: "fs_read_text" }),
      item({ id: "fs_write_text", destructive: true, requires_approval: true }),
      item({ id: "browser_open", family: "browser", available: false }),
      item({ id: "mcp__sb__read_note", family: "mcp", origin: "mcp" })
    ]),
    {
      total: 4,
      builtIn: 3,
      mcp: 1,
      destructive: 1,
      unavailable: 1
    }
  );
});

test("agentAllowsTool supports direct ids and MCP aggregate allowlist entries", () => {
  assert.equal(agentAllowsTool(["fs_read_text"], "fs_read_text"), true);
  assert.equal(agentAllowsTool(["mcp"], "mcp__silverbullet__read_note"), true);
  assert.equal(agentAllowsTool(["mcp_call"], "mcp__silverbullet__read_note"), true);
  assert.equal(agentAllowsTool(["web_search"], "mcp__silverbullet__read_note"), false);
});
