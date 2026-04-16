import { describe, expect, it } from "vitest";
import { buildControlHeaderView, tabs } from "./layout";
import type { BootstrapPayload, SessionSnapshot } from "./lib/types";

function makeBootstrap(): BootstrapPayload {
  return {
    agent_id: "zai-smoke",
    config_path: "./config/zai-smoke/agent.yaml",
    listen_addr: "0.0.0.0:18080",
    generated_at: "2026-04-16T05:10:00Z",
    transport: { endpoint_path: "/api", websocket_path: "/ws" },
    assets: { mode: "embedded_assets" },
    settings: { revision: "rev-1", form_fields: [], raw_files: [] },
    sessions: [],
  };
}

function makeSession(): SessionSnapshot {
  return {
    session_id: "session-1",
    created_at: "2026-04-16T05:10:00Z",
    last_activity: "2026-04-16T05:10:00Z",
    message_count: 3,
    main_run_active: false,
    main_run: { active: false, started_at: "", provider: "api.z.ai", model: "glm-5-turbo", input_tokens: 0, output_tokens: 0, total_tokens: 0 },
    queued_drafts: [],
    history: { loaded_count: 0, total_count: 0, has_more: false, window_limit: 40 },
    base_context_tokens: 0,
    transcript: [],
    timeline: [],
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
  };
}

describe("layout helpers", () => {
  it("builds a control header view with active session and daemon chips", () => {
    const header = buildControlHeaderView({
      bootstrap: makeBootstrap(),
      connected: true,
      selectedSession: makeSession(),
    });

    expect(header.sessionLabel).toBe("session-1");
    expect(header.sessionMeta).toContain("3 messages");
    expect(header.statusChips).toContain("websocket up");
    expect(header.statusChips).toContain("api.z.ai");
    expect(header.statusChips).toContain("glm-5-turbo");
  });

  it("keeps sessions and chat as first-class tabs", () => {
    expect(tabs.map((tab) => tab.key)).toEqual(["sessions", "chat", "plan", "tools", "settings"]);
  });
});
