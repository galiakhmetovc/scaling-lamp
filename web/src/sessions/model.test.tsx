import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { buildSessionList, formatLastActivity, sessionSelectionIntent } from "./model";
import { SessionsPane } from "./SessionsPane";
import type { BootstrapPayload, SessionSummary } from "../lib/types";

const sessions: SessionSummary[] = [
  { session_id: "session-older", created_at: "2026-04-16T05:00:00Z", last_activity: "2026-04-16T05:02:00Z", message_count: 2 },
  { session_id: "session-newer", created_at: "2026-04-16T05:01:00Z", last_activity: "2026-04-16T05:03:00Z", message_count: 4 },
];

const bootstrap: BootstrapPayload = {
  agent_id: "zai-smoke",
  config_path: "./config/zai-smoke/agent.yaml",
  listen_addr: "0.0.0.0:18080",
  artifact_store_path: "/tmp/artifacts",
  generated_at: "2026-04-16T05:10:00Z",
  transport: { endpoint_path: "/api", websocket_path: "/ws" },
  assets: { mode: "embedded_assets" },
  settings: { revision: "rev-1", form_fields: [], quick_controls: [], raw_files: [] },
  sessions,
};

describe("sessions model", () => {
  it("sorts by last activity and marks the active session", () => {
    const list = buildSessionList(sessions, "session-newer");
    expect(list[0].id).toBe("session-newer");
    expect(list[0].active).toBe(true);
    expect(list[1].active).toBe(false);
    expect(list[0].activityText).toContain("active");
  });

  it("formats last activity text for compact session cards", () => {
    expect(formatLastActivity("2026-04-16T05:03:00Z")).toContain("active");
  });

  it("switches into chat when a session is selected from the catalog", () => {
    expect(sessionSelectionIntent("session-newer")).toEqual({
      sessionID: "session-newer",
      nextTab: "chat",
    });
  });
});

describe("SessionsPane", () => {
  it("renders a standalone catalog and control plane panel", () => {
    const markup = renderToStaticMarkup(
      <SessionsPane
        bootstrap={bootstrap}
        sessions={sessions}
        selectedSessionID="session-newer"
        onSelectSession={() => {}}
        onCreateSession={() => {}}
      />,
    );

    expect(markup).toContain("Session catalog");
    expect(markup).toContain("Control plane");
    expect(markup).toContain("session-newer");
    expect(markup).toContain("/tmp/artifacts");
    expect(markup).toContain("surface-primary");
    expect(markup).toContain("surface-secondary");
    expect(markup).toContain("active");
  });
});
