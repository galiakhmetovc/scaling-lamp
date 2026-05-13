import type { AgentSummary, DeliveryTarget, RunSummary, SessionSummary, SessionTask, TelegramChat, WebSnapshot } from "../../types";

export type AgentLane = {
  agent: AgentSummary;
  sessions: SessionSummary[];
  activeRuns: RunSummary[];
  tasks: SessionTask[];
  activeTasks: number;
  failedTasks: number;
  telegramChats: TelegramChat[];
  deliveryTargets: DeliveryTarget[];
  lastUpdated: number;
};

const ACTIVE_STATUSES = new Set(["active", "queued", "running", "in_progress"]);
const FAILED_STATUSES = new Set(["failed", "error", "cancelled", "timeout"]);

export function isActiveStatus(status: string | null | undefined): boolean {
  return ACTIVE_STATUSES.has((status ?? "").toLowerCase());
}

export function isFailedStatus(status: string | null | undefined): boolean {
  return FAILED_STATUSES.has((status ?? "").toLowerCase());
}

export function buildAgentLanes(snapshot: WebSnapshot): AgentLane[] {
  const recentTasks = snapshot.recent_tasks ?? [];
  return snapshot.agents
    .map((agent) => {
      const sessions = snapshot.sessions.filter((session) => session.agent_profile_id === agent.id);
      const sessionIds = new Set(sessions.map((session) => session.id));
      const tasks = recentTasks.filter(
        (task) => task.owner_agent_id === agent.id || task.executor_agent_id === agent.id || (task.source_session_id && sessionIds.has(task.source_session_id))
      );
      const activeRuns = snapshot.recent_runs.filter((run) => sessionIds.has(run.session_id) && isActiveStatus(run.status));
      const telegramChats = snapshot.telegram_chats.filter((chat) => chat.default_agent_profile_id === agent.id);
      const deliveryTargets = snapshot.delivery_targets.filter((target) => target.scope === agent.id || target.target_id.includes(agent.id));
      const lastUpdated = Math.max(
        agent.updated_at,
        ...sessions.map((session) => session.updated_at),
        ...tasks.map((task) => task.updated_at),
        ...activeRuns.map((run) => run.updated_at),
        0
      );
      return {
        agent,
        sessions,
        activeRuns,
        tasks,
        activeTasks: tasks.filter((task) => isActiveStatus(task.status)).length,
        failedTasks: tasks.filter((task) => isFailedStatus(task.status)).length,
        telegramChats,
        deliveryTargets,
        lastUpdated
      };
    })
    .sort(
      (left, right) =>
        right.activeTasks - left.activeTasks ||
        right.activeRuns.length - left.activeRuns.length ||
        right.sessions.length - left.sessions.length ||
        left.agent.name.localeCompare(right.agent.name)
    );
}

export function countTasksByStatus(tasks: SessionTask[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const task of tasks) {
    counts[task.status] = (counts[task.status] ?? 0) + 1;
  }
  return counts;
}
