import type { BootstrapPayload, SessionSnapshot } from "./lib/types";

export type TabKey = "sessions" | "chat" | "plan" | "tools" | "settings";

export const tabs: Array<{ key: TabKey; label: string }> = [
  { key: "sessions", label: "Sessions" },
  { key: "chat", label: "Chat" },
  { key: "plan", label: "Plan" },
  { key: "tools", label: "Tools" },
  { key: "settings", label: "Settings" },
];

export type ControlHeaderView = {
  eyebrow: string;
  title: string;
  sessionLabel: string;
  sessionMeta: string;
  statusChips: string[];
};

export function buildControlHeaderView(args: {
  bootstrap: BootstrapPayload | null;
  connected: boolean;
  selectedSession: SessionSnapshot | null;
  errorMessage?: string;
}): ControlHeaderView {
  const { bootstrap, connected, selectedSession, errorMessage } = args;
  const sessionLabel = selectedSession ? (selectedSession.title || selectedSession.session_id) : "No active session";
  const sessionMeta = selectedSession
    ? `${selectedSession.message_count} messages · ${selectedSession.session_id}`
    : "Create or select a session to start working";
  const statusChips = [
    connected ? "websocket up" : "websocket down",
    bootstrap?.agent_id ?? "agent loading",
    bootstrap?.listen_addr ?? "listen pending",
  ];
  if (selectedSession) {
    statusChips.push(selectedSession.main_run.provider);
    statusChips.push(selectedSession.main_run.model);
  }
  if (errorMessage) {
    statusChips.push(errorMessage);
  }
  return {
    eyebrow: "teamD operator surface",
    title: "Daemon Console",
    sessionLabel,
    sessionMeta,
    statusChips,
  };
}
