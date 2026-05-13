import test from "node:test";
import assert from "node:assert/strict";
import {
  getPromptProfileFiles,
  getSkillProfileFiles,
  skillNameFromPath,
  type SkillProfileFileEntry
} from "./skillProfileFiles.ts";

function file(path: string, kind = "prompt"): SkillProfileFileEntry {
  return { path, kind, byte_len: 10 };
}

test("getPromptProfileFiles returns only SYSTEM.md and AGENTS.md in fixed order", () => {
  const files = [
    file("skills/weather/SKILL.md", "skill"),
    file("AGENTS.md"),
    file("README.md"),
    file("SYSTEM.md")
  ];

  assert.deepEqual(
    getPromptProfileFiles(files).map((entry) => entry.path),
    ["SYSTEM.md", "AGENTS.md"]
  );
});

test("getSkillProfileFiles returns only skill documents sorted by skill name", () => {
  const files = [
    file("SYSTEM.md"),
    file("skills/zeta/SKILL.md", "skill"),
    file("skills/alpha/SKILL.md", "skill"),
    file("skills/alpha/README.md", "skill")
  ];

  assert.deepEqual(
    getSkillProfileFiles(files).map((entry) => entry.path),
    ["skills/alpha/SKILL.md", "skills/zeta/SKILL.md"]
  );
});

test("skillNameFromPath extracts skill directory name", () => {
  assert.equal(skillNameFromPath("skills/weather/SKILL.md"), "weather");
  assert.equal(skillNameFromPath("SYSTEM.md"), null);
});
