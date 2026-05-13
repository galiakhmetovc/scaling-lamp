import test from "node:test";
import assert from "node:assert/strict";
import {
  buildWorkspaceTreeNodes,
  getParentPath,
  joinWorkspacePath,
  type WorkspaceTreeEntry
} from "./workspaceTree.ts";

function entry(path: string, kind: WorkspaceTreeEntry["kind"], bytes?: number): WorkspaceTreeEntry {
  return { path, kind, bytes };
}

test("joinWorkspacePath joins root and nested paths without duplicate slashes", () => {
  assert.equal(joinWorkspacePath("", "notes"), "notes");
  assert.equal(joinWorkspacePath("notes", "daily.md"), "notes/daily.md");
  assert.equal(joinWorkspacePath("notes/", "/daily.md"), "notes/daily.md");
});

test("getParentPath returns workspace parent or root", () => {
  assert.equal(getParentPath("notes/daily.md"), "notes");
  assert.equal(getParentPath("notes"), "");
  assert.equal(getParentPath(""), "");
});

test("buildWorkspaceTreeNodes sorts directories before files by basename", () => {
  const nodes = buildWorkspaceTreeNodes([
    entry("z.md", "file", 12),
    entry("notes", "directory"),
    entry("a.md", "file", 3),
    entry("archive", "directory")
  ]);

  assert.deepEqual(
    nodes.map((node) => node.path),
    ["archive", "notes", "a.md", "z.md"]
  );
  assert.equal(nodes[0].depth, 0);
  assert.equal(nodes[2].label, "a.md");
});
