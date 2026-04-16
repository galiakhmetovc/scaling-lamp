import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ChatPane } from "./ChatPane";
import { approximateContextTokens, applySessionUIEvent, buildChatStatus, buildBtwRuns, buildLiveToolView, buildTimelineMarkdownBlock, syncMainRunFromUIEvent } from "./model";
import type { ProviderResultPayload, SessionSnapshot } from "../lib/types";
import type { SettingsFieldState } from "../lib/types";

function makeSessionSnapshot(overrides: Partial<SessionSnapshot> = {}): SessionSnapshot {
  return {
    session_id: "session-1",
    created_at: "2026-04-15T21:00:00Z",
    last_activity: "2026-04-15T21:01:00Z",
    message_count: 2,
    main_run_active: false,
    main_run: {
      active: false,
      started_at: "",
      provider: "provider.example.test",
      model: "glm-5-turbo",
      input_tokens: 8,
      output_tokens: 4,
      total_tokens: 12,
    },
    queued_drafts: [],
    history: {
      loaded_count: 3,
      total_count: 3,
      has_more: false,
      window_limit: 40,
    },
    base_context_tokens: 7,
    transcript: [
      { role: "user", content: "hello" },
      { role: "assistant", content: "world" },
    ],
    timeline: [
      { kind: "message", role: "user", content: "hello" },
      { kind: "tool", content: "**Tool result** `shell_exec`\n\n`stdout line`" },
      { kind: "plan", content: "**Task added**\n\n`Ship daemon UI`" },
    ],
    plan: {
      plan: { id: "", goal: "", status: "", created_at: "" },
      tasks: {},
      ready: {},
      waiting_on_dependencies: {},
      blocked: {},
      notes: {},
    },
    pending_approvals: [],
    running_commands: [],
    delegates: [],
    ...overrides,
  };
}

const quickControls: SettingsFieldState[] = [
  { key: "model", label: "Model", type: "string", value: "glm-5-turbo", file_path: "policies/request-shape/model.yaml", revision: "rev-1", enum: ["glm-5-turbo", "glm-4.6"] },
  { key: "allow_network", label: "Network", type: "bool", value: true, file_path: "policies/tool-execution/sandbox.yaml", revision: "rev-2" },
];

describe("chat model helpers", () => {
  it("counts base context tokens, input, and queued drafts for approximate context tokens", () => {
    const snapshot = makeSessionSnapshot({
      base_context_tokens: 12,
      queued_drafts: [{ id: "draft-1", text: "queued text", queued_at: "2026-04-15T21:01:30Z" }],
    });

    expect(approximateContextTokens(snapshot, "typing now")).toBeGreaterThanOrEqual(17);
  });

  it("builds a status bar model from main run metadata and btw runs", () => {
    const snapshot = makeSessionSnapshot({
      main_run_active: true,
      main_run: {
        active: true,
        started_at: "2026-04-15T21:00:00Z",
        provider: "provider.example.test",
        model: "glm-5-turbo",
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
      },
      queued_drafts: [{ id: "draft-1", text: "queued text", queued_at: "2026-04-15T21:01:30Z" }],
    });

    const status = buildChatStatus({
      session: snapshot,
      input: "drafting",
      now: new Date("2026-04-15T21:01:20Z"),
      activeBtwCount: 1,
    });

    expect(status.provider).toBe("provider.example.test");
    expect(status.model).toBe("glm-5-turbo");
    expect(status.runText).toBe("running 01:20");
    expect(status.queueCount).toBe(1);
    expect(status.activeBtwCount).toBe(1);
    expect(status.contextTokens).toBeGreaterThan(0);
    expect("currentTime" in status).toBe(false);
  });

  it("maps btw runs into isolated branch state", () => {
    const result: ProviderResultPayload = {
      provider: "provider.example.test",
      model: "glm-5-turbo",
      input_tokens: 8,
      output_tokens: 4,
      total_tokens: 12,
      content: "side answer",
    };

    const runs = buildBtwRuns([
      { id: "btw-1", prompt: "question", active: false, result },
      { id: "btw-2", prompt: "pending", active: true },
    ]);

    expect(runs[0].body).toContain("side answer");
    expect(runs[1].statusText).toBe("running");
  });

  it("splits tool timeline content into collapsible summary and body", () => {
    const block = buildTimelineMarkdownBlock("tool", "**Tool result** `shell_exec`\n\n`line one\\nline two`");
    expect(block.collapsible).toBe(true);
    expect(block.summary).toContain("Tool result");
    expect(block.body).toContain("line one");
  });

  it("builds compact live tool views and marks approval-required tools as errors", () => {
    const approvalView = buildLiveToolView({
      phase: "completed",
      name: "shell_start",
      arguments: { command: "rm" },
      error_text: `tool call "shell_start" requires approval`,
      result_text: `{"status":"approval_pending"}`,
    });
    expect(approvalView.summary).toContain("Approval required");
    expect(approvalView.collapsible).toBe(true);
    expect(approvalView.state).toBe("error");
  });

  it("marks the main run active when a running status event arrives", () => {
    const snapshot = makeSessionSnapshot();
    const next = syncMainRunFromUIEvent(
      snapshot,
      { kind: "status.changed", session_id: "session-1", run_id: "run-1", text: "", status: "running" },
      new Date("2026-04-15T21:01:20Z"),
    );

    expect(next?.main_run.active).toBe(true);
    expect(next?.main_run.started_at).toBe("2026-04-15T21:01:20.000Z");
  });

  it("keeps streaming text and clears it on run completion", () => {
    const running = applySessionUIEvent(undefined, {
      kind: "stream.text",
      session_id: "session-1",
      run_id: "run-1",
      text: "partial",
      status: "",
    });
    expect(running.streaming).toBe("partial");

    const completed = applySessionUIEvent(running, {
      kind: "run.completed",
      session_id: "session-1",
      run_id: "run-1",
      text: "",
      status: "done",
    });
    expect(completed.streaming).toBe("");
    expect(completed.status).toBe("done");
  });

});

describe("ChatPane", () => {
  it("renders a chat-first workspace with primary timeline and operational sidebar", () => {
    const markup = renderToStaticMarkup(
      <ChatPane
        session={makeSessionSnapshot({
          queued_drafts: [{ id: "draft-1", text: "queued text", queued_at: "2026-04-15T21:01:30Z" }],
        })}
        streaming="streaming text"
        status="ready"
        input="hello"
        now={new Date("2026-04-15T21:01:20Z")}
        btwRuns={[
          {
            id: "btw-1",
            prompt: "question",
            active: false,
            result: {
              provider: "provider.example.test",
              model: "glm-5-turbo",
              input_tokens: 8,
              output_tokens: 4,
              total_tokens: 12,
              content: "branch answer",
            },
          },
        ]}
        toolLog={[
          { phase: "started", name: "fs_list", arguments: { path: "." } },
          { phase: "completed", name: "shell_start", arguments: { command: "curl" }, error_text: `tool call "shell_start" requires approval`, result_text: `{"status":"approval_pending"}` },
        ]}
        onInput={() => {}}
        onSend={() => {}}
        onQueue={() => {}}
        onRecallDraft={() => {}}
        onLoadOlder={() => {}}
        quickControls={quickControls}
        settingsError=""
        onQuickControlChange={() => {}}
      />,
    );

    expect(markup).toContain("provider.example.test");
    expect(markup).toContain("glm-5-turbo");
    expect(markup).toContain("queued text");
    expect(markup).toContain("/btw");
    expect(markup).toContain("branch answer");
    expect(markup).toContain("shell_exec");
    expect(markup).toContain("tool-result-toggle");
    expect(markup).toContain("<summary>");
    expect(markup).toContain("Approval required");
    expect(markup).toContain("Tool started");
    expect(markup).not.toContain("time ");
    expect(markup).toContain("chat-workspace");
    expect(markup).toContain("surface-primary");
    expect(markup).toContain("surface-secondary");
    expect(markup).toContain("timeline-scroll");
    expect(markup).toContain("ops-scroll");
    expect(markup).toContain("Run controls");
    expect(markup).toContain("allow_network");
  });

  it("renders a load older affordance when more history is available", () => {
    const markup = renderToStaticMarkup(
      <ChatPane
        session={makeSessionSnapshot({
          history: {
            loaded_count: 40,
            total_count: 52,
            has_more: true,
            window_limit: 40,
          },
        })}
        streaming=""
        status="ready"
        input=""
        now={new Date("2026-04-15T21:01:20Z")}
        btwRuns={[]}
        toolLog={[]}
        onInput={() => {}}
        onSend={() => {}}
        onQueue={() => {}}
        onRecallDraft={() => {}}
        onLoadOlder={() => {}}
        quickControls={[]}
        settingsError=""
        onQuickControlChange={() => {}}
      />,
    );

    expect(markup).toContain("Load older");
    expect(markup).toContain("12 older");
  });
});
