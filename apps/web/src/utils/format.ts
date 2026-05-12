import type { ChipProps } from "@mui/material";

export function formatTime(value?: number | null): string {
  if (!value) {
    return "—";
  }
  return new Date(value * 1000).toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  });
}

export function short(value?: string | null, size = 14): string {
  if (!value) {
    return "—";
  }
  return value.length > size ? `${value.slice(0, size)}…` : value;
}

export function statusColor(status?: string | null): ChipProps["color"] {
  const normalized = (status ?? "").toLowerCase();
  if (["completed", "done", "ok", "idle", "ready", "success"].includes(normalized)) {
    return "success";
  }
  if (["running", "queued", "in_progress", "active"].includes(normalized)) {
    return "warning";
  }
  if (["failed", "error", "cancelled", "killed"].includes(normalized)) {
    return "error";
  }
  return "default";
}

export function parseJsonLabel(value?: string | null): string {
  if (!value) {
    return "—";
  }
  try {
    const parsed = JSON.parse(value);
    if (typeof parsed === "string") {
      return parsed;
    }
    if (parsed.goal) {
      return String(parsed.goal);
    }
    if (parsed.prompt) {
      return String(parsed.prompt);
    }
    return JSON.stringify(parsed);
  } catch {
    return value;
  }
}
