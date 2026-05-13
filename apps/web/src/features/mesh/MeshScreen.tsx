import { Button, Chip, Paper, Stack, Typography } from "@mui/material";
import { EmptyState, Metric, SectionHeader } from "../../components/common";
import type { WebSnapshot } from "../../types";
import { MeshAgentLanes } from "./MeshAgentLanes";
import { MeshRoutesPanel } from "./MeshRoutesPanel";
import { MeshTasksTable } from "./MeshTasksTable";
import { buildAgentLanes, countTasksByStatus, isActiveStatus, isFailedStatus } from "./meshModel";

export function MeshScreen({
  snapshot,
  loading,
  onRefresh,
  onOpenSession
}: {
  snapshot: WebSnapshot | null;
  loading: boolean;
  onRefresh: () => void;
  onOpenSession: (sessionId: string) => void;
}) {
  if (!snapshot) {
    return <EmptyState title="Mesh snapshot недоступен" detail="Нет данных от /v1/web/snapshot." />;
  }

  const lanes = buildAgentLanes(snapshot);
  const recentTasks = snapshot.recent_tasks ?? [];
  const activeTasks = recentTasks.filter((task) => isActiveStatus(task.status)).length;
  const failedTasks = recentTasks.filter((task) => isFailedStatus(task.status)).length;
  const taskStatusCounts = countTasksByStatus(recentTasks);

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Mesh / Swarm"
        subtitle="Read-only карта MIMO runtime: agent lanes, task registry, delivery routes, Telegram inputs и NATS streams."
        action={
          <Button variant="outlined" onClick={onRefresh} disabled={loading}>
            Обновить
          </Button>
        }
      />

      <Stack direction="row" spacing={1.5} flexWrap="wrap" useFlexGap>
        <Metric label="Agents" value={snapshot.agents.length} />
        <Metric label="Sessions in snapshot" value={snapshot.sessions.length} />
        <Metric label="Recent tasks" value={recentTasks.length} hint={`${activeTasks} active · ${failedTasks} failed`} />
        <Metric label="Delivery targets" value={snapshot.delivery_targets.length} />
        <Metric label="Telegram inputs" value={snapshot.telegram_chats.length} />
        <Metric label="NATS" value={snapshot.event_bus.nats_configured ? "configured" : "missing"} />
      </Stack>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Task status mix
        </Typography>
        <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
          {Object.entries(taskStatusCounts).map(([status, count]) => (
            <Chip key={status} label={`${status}: ${count}`} variant="outlined" />
          ))}
          {recentTasks.length === 0 ? (
            <Typography variant="body2" color="text.secondary">
              Нет recent task registry записей.
            </Typography>
          ) : null}
        </Stack>
      </Paper>

      <MeshAgentLanes lanes={lanes} onOpenSession={onOpenSession} />
      <MeshTasksTable tasks={recentTasks} sessions={snapshot.sessions} agents={snapshot.agents} onOpenSession={onOpenSession} />
      <MeshRoutesPanel
        eventBus={snapshot.event_bus}
        targets={snapshot.delivery_targets}
        chats={snapshot.telegram_chats}
        sessions={snapshot.sessions}
        agents={snapshot.agents}
        onOpenSession={onOpenSession}
      />
    </Stack>
  );
}
