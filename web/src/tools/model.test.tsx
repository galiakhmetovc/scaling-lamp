import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ToolsPane } from "./ToolsPane";
import { reverseToolLog } from "./model";

describe("tools model", () => {
  it("reverses tool log for newest-first rendering", () => {
    expect(reverseToolLog([{ name: "one", phase: "started", arguments: {} }, { name: "two", phase: "completed", arguments: {} }]).map((item) => item.name)).toEqual(["two", "one"]);
  });
});

describe("ToolsPane", () => {
  it("renders approvals, running commands, delegates, and tool log with tiered surfaces", () => {
    const markup = renderToStaticMarkup(
      <ToolsPane
        approvals={[{ approval_id: "ap-1", command_id: "cmd-1", tool_name: "shell_start", message: "Need approval", command: "git", args: ["push"], cwd: "/repo" }]}
        commands={[{ command_id: "cmd-2", session_id: "session-1", run_id: "run-1", command: "go", args: ["test", "./..."], status: "running", next_offset: 1, last_chunk: "PASS", kill_pending: false }]}
        toolLog={[{ name: "shell_start", phase: "completed", arguments: {}, result_text: "started" }]}
        delegates={[{ delegate_id: "delegate-1", session_id: "session-1", status: "running", task: "Investigate" }]}
        onApprove={() => {}}
        onDeny={() => {}}
        onKill={() => {}}
      />,
    );

    expect(markup).toContain("Need approval");
    expect(markup).toContain("git");
    expect(markup).toContain("PASS");
    expect(markup).toContain("delegate-1");
    expect(markup).toContain("started");
    expect(markup).toContain("surface-primary");
    expect(markup).toContain("surface-secondary");
  });
});
