import { strict as assert } from "node:assert";
import { describe, it } from "node:test";
import { describeMemoryLayer, jsonPreview, memoryScopeRequiresSession, parseJsonInput } from "./memoryModel.ts";

describe("memoryModel", () => {
  it("renders bounded JSON previews", () => {
    assert.equal(jsonPreview({ a: 1 }), "{\"a\":1}");
    assert.equal(jsonPreview({ text: "abcdef" }, 12), "{\"text\":\"abc…");
  });

  it("parses blank JSON input as null", () => {
    assert.equal(parseJsonInput("  "), null);
    assert.deepEqual(parseJsonInput("{\"ok\":true}"), { ok: true });
  });

  it("describes durable data layers distinctly", () => {
    assert.match(describeMemoryLayer("mem0"), /семантическая/);
    assert.match(describeMemoryLayer("kv"), /key-value/);
    assert.match(describeMemoryLayer("silverbullet"), /заметки/);
  });

  it("distinguishes global and session-context memory scopes", () => {
    assert.equal(memoryScopeRequiresSession("operator"), false);
    assert.equal(memoryScopeRequiresSession("agent_shared"), false);
    assert.equal(memoryScopeRequiresSession("workspace"), true);
    assert.equal(memoryScopeRequiresSession("agent"), true);
    assert.equal(memoryScopeRequiresSession("session"), true);
  });
});
