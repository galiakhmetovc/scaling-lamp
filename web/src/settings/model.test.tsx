import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SettingsPane } from "./SettingsPane";
import { detectSettingsDirty, isRevisionConflict } from "./model";
import type { SettingsRawFileContent, SettingsSnapshot } from "../lib/types";

const settings: SettingsSnapshot = {
  revision: "rev-1",
  form_fields: [
    { key: "model", label: "Model", type: "string", value: "glm-5-turbo", file_path: "policies/request-shape/model.yaml", revision: "rev-model", enum: ["glm-5-turbo", "glm-4.6"] },
    { key: "markdown_style", label: "Markdown Style", type: "string", value: "dark", file_path: "policies/chat/output.yaml", revision: "rev-a", enum: ["dark", "light"] },
    { key: "show_tool_calls", label: "Show Tool Calls", type: "bool", value: true, file_path: "policies/chat/status.yaml", revision: "rev-b" },
  ],
  quick_controls: [
    { key: "model", label: "Model", type: "string", value: "glm-5-turbo", file_path: "policies/request-shape/model.yaml", revision: "rev-model", enum: ["glm-5-turbo", "glm-4.6"] },
  ],
  raw_files: [{ path: "policies/chat/output.yaml", revision: "raw-1", size: 12 }],
};

const rawFile: SettingsRawFileContent = {
  path: "policies/chat/output.yaml",
  revision: "raw-1",
  content: "kind: ChatOutputPolicyConfig",
};

describe("settings model", () => {
  it("detects dirty form and raw state", () => {
    expect(detectSettingsDirty(settings, { markdown_style: "light", show_tool_calls: true }, rawFile, rawFile.content)).toEqual({
      formDirty: true,
      rawDirty: false,
    });
  });

  it("detects revision conflicts from daemon errors", () => {
    expect(isRevisionConflict("settings raw revision conflict")).toBe(true);
    expect(isRevisionConflict("boom")).toBe(false);
  });
});

describe("SettingsPane", () => {
  it("renders revision, conflict UI, and tiered surfaces", () => {
    const markup = renderToStaticMarkup(
      <SettingsPane
        settings={settings}
        draft={{ markdown_style: "light", show_tool_calls: true }}
        rawFile={rawFile}
        selectedRawPath={rawFile.path}
        rawDraft={rawFile.content}
        error="settings revision conflict"
        onDraftChange={() => {}}
        onApply={() => {}}
        onSelectRaw={() => {}}
        onRawChange={() => {}}
        onApplyRaw={() => {}}
      />,
    );

    expect(markup).toContain("rev-1");
    expect(markup).toContain("dirty");
    expect(markup).toContain("revision conflict");
    expect(markup).toContain("policies/chat/output.yaml");
    expect(markup).toContain("surface-primary");
    expect(markup).toContain("surface-secondary");
    expect(markup).toContain("select");
  });
});
