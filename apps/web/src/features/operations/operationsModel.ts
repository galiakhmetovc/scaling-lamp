import type { RunSummary, SessionTask, WebSnapshot } from "../../types";

const ACTIVE_STATUSES = new Set(["running", "queued", "in_progress", "active", "pending"]);
const FAILED_STATUSES = new Set(["failed", "error", "cancelled", "killed"]);

export type OperationsSummary = {
  activeRuns: number;
  failedRuns: number;
  activeTasks: number;
  failedTasks: number;
  deliveryTargets: number;
  telegramInputs: number;
  dlqStream: string;
};

export function isActiveOperationsStatus(status?: string | null): boolean {
  return ACTIVE_STATUSES.has((status ?? "").toLowerCase());
}

export function isFailedOperationsStatus(status?: string | null): boolean {
  return FAILED_STATUSES.has((status ?? "").toLowerCase());
}

export function summarizeOperations(snapshot: WebSnapshot): OperationsSummary {
  return {
    activeRuns: snapshot.recent_runs.filter((run) => isActiveOperationsStatus(run.status)).length,
    failedRuns: snapshot.recent_runs.filter((run) => isFailedOperationsStatus(run.status)).length,
    activeTasks: snapshot.recent_tasks.filter((task) => isActiveOperationsStatus(task.status)).length,
    failedTasks: snapshot.recent_tasks.filter((task) => isFailedOperationsStatus(task.status)).length,
    deliveryTargets: snapshot.delivery_targets.length,
    telegramInputs: snapshot.telegram_chats.length,
    dlqStream: snapshot.event_bus.dlq_stream
  };
}

export function recentActiveRuns(runs: RunSummary[]): RunSummary[] {
  return runs.filter((run) => isActiveOperationsStatus(run.status));
}

export function recentActiveTasks(tasks: SessionTask[]): SessionTask[] {
  return tasks.filter((task) => isActiveOperationsStatus(task.status));
}
