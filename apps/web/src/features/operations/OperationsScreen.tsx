import { Alert, Button, Chip, Paper, Stack, Typography } from "@mui/material";
import { EmptyState, Metric, SectionHeader } from "../../components/common";
import type { WebSnapshot } from "../../types";
import { RunsTable } from "../runs/RunsTable";
import { MeshRoutesPanel } from "../mesh/MeshRoutesPanel";
import { MeshTasksTable } from "../mesh/MeshTasksTable";
import { recentActiveRuns, recentActiveTasks, summarizeOperations } from "./operationsModel";

export function OperationsScreen({
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
    return <EmptyState title="Operations snapshot недоступен" detail="Нет данных от /v1/web/snapshot." />;
  }

  const summary = summarizeOperations(snapshot);
  const activeRuns = recentActiveRuns(snapshot.recent_runs);
  const activeTasks = recentActiveTasks(snapshot.recent_tasks);

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Operations"
        subtitle="Операционный экран поверх snapshot: активная работа, task registry, delivery routes и event bus."
        action={
          <Button variant="outlined" onClick={onRefresh} disabled={loading}>
            Обновить
          </Button>
        }
      />

      <Stack direction="row" spacing={1.5} flexWrap="wrap" useFlexGap>
        <Metric label="Active runs" value={summary.activeRuns} hint={`${summary.failedRuns} failed recent`} />
        <Metric label="Active tasks" value={summary.activeTasks} hint={`${summary.failedTasks} failed recent`} />
        <Metric label="Delivery targets" value={summary.deliveryTargets} />
        <Metric label="Telegram inputs" value={summary.telegramInputs} />
        <Metric label="DLQ stream" value={summary.dlqStream} />
      </Stack>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Event bus
        </Typography>
        <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
          <Chip label={`backend: ${snapshot.event_bus.backend}`} color={snapshot.event_bus.nats_configured ? "success" : "warning"} variant="outlined" />
          <Chip label={`required: ${snapshot.event_bus.required ? "yes" : "no"}`} variant="outlined" />
          <Chip label={`input: ${snapshot.event_bus.input_stream}`} variant="outlined" />
          <Chip label={`session: ${snapshot.event_bus.session_stream}`} variant="outlined" />
          <Chip label={`task: ${snapshot.event_bus.task_stream}`} variant="outlined" />
          <Chip label={`delivery: ${snapshot.event_bus.delivery_stream}`} variant="outlined" />
          <Chip label={`dlq: ${snapshot.event_bus.dlq_stream}`} color="warning" variant="outlined" />
        </Stack>
      </Paper>

      {summary.failedRuns || summary.failedTasks ? (
        <Alert severity="warning">
          Есть последние failed/cancelled сущности. Смотри таблицы ниже и session debug для конкретного run/task.
        </Alert>
      ) : null}

      <SectionHeader title="Active runs" subtitle="Только runs со статусом running/queued/in_progress/active/pending." />
      <RunsTable runs={activeRuns.length ? activeRuns : snapshot.recent_runs.slice(0, 10)} />

      <SectionHeader title="Active tasks" subtitle="Task registry: agent_task/delegate и фоновые задачи, видимые в snapshot." />
      <MeshTasksTable tasks={activeTasks.length ? activeTasks : snapshot.recent_tasks.slice(0, 10)} onOpenSession={onOpenSession} />

      <SectionHeader title="Routing" subtitle="Fan-out targets и Telegram fan-in bindings." />
      <MeshRoutesPanel eventBus={snapshot.event_bus} targets={snapshot.delivery_targets} chats={snapshot.telegram_chats} />
    </Stack>
  );
}
