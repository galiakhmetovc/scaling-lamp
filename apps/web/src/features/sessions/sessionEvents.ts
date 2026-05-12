import type { SessionDebug, SessionTranscript } from "../../types";

export type SessionPane = "timeline" | "transcript" | "debug" | "tasks" | "run";

export type SessionEvent = {
  id: string;
  kind: string;
  label: string;
  detailTitle: string;
  detail: string;
  createdAt: number;
  runId?: string | null;
  artifactId?: string | null;
  source: "debug" | "transcript";
};

export function eventTone(kind: string): "primary" | "secondary" | "warning" | "success" | "error" | "default" {
  const normalized = kind.toLowerCase();
  if (normalized.includes("tool")) {
    return "warning";
  }
  if (normalized.includes("artifact")) {
    return "secondary";
  }
  if (normalized.includes("assistant")) {
    return "primary";
  }
  if (normalized.includes("user")) {
    return "success";
  }
  if (normalized.includes("error") || normalized.includes("failed")) {
    return "error";
  }
  return "default";
}

export function buildSessionEvents(debug: SessionDebug | null, transcript: SessionTranscript | null): SessionEvent[] {
  if (debug?.entries.length) {
    return debug.entries
      .map((entry) => ({
        id: entry.id,
        kind: entry.kind,
        label: entry.label,
        detailTitle: entry.detail_title,
        detail: entry.detail,
        createdAt: entry.created_at,
        runId: entry.run_id,
        artifactId: entry.artifact_id,
        source: "debug" as const
      }))
      .sort((left, right) => left.createdAt - right.createdAt || left.id.localeCompare(right.id));
  }

  return (transcript?.entries ?? [])
    .map((entry, index) => ({
      id: `transcript-${entry.created_at}-${index}`,
      kind: entry.tool_name ? "tool" : entry.role,
      label: entry.tool_name ?? entry.role,
      detailTitle: entry.tool_status ? `${entry.tool_name ?? "tool"} · ${entry.tool_status}` : entry.role,
      detail: entry.content,
      createdAt: entry.created_at,
      runId: entry.run_id,
      source: "transcript" as const
    }))
    .sort((left, right) => left.createdAt - right.createdAt || left.id.localeCompare(right.id));
}
