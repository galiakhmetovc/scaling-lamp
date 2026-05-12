import { Box, Button, Chip, Divider, Paper, Stack, Typography } from "@mui/material";
import { EmptyState, JsonBlock, KeyValueTable, StatusChip } from "../../components/common";
import type { SessionSummary, SessionTask, ToolCallSummary } from "../../types";
import { formatTime, short } from "../../utils/format";
import { eventTone, type SessionEvent } from "./sessionEvents";

export function SessionInspector({
  session,
  selectedEvent,
  tasks,
  tools,
  run,
  onRefresh,
  onCancelRun,
  onCancelAll
}: {
  session: SessionSummary | null;
  selectedEvent: SessionEvent | null;
  tasks: SessionTask[];
  tools: ToolCallSummary[];
  run: unknown;
  onRefresh: () => void;
  onCancelRun: () => void;
  onCancelAll: () => void;
}) {
  if (!session) {
    return <EmptyState title="Inspector пуст" detail="Выбери сессию." />;
  }

  const sessionTools = tools.filter((tool) => tool.session_id === session.id);
  const failedTools = sessionTools.filter((tool) => tool.status !== "completed" || tool.error);
  const activeTasks = tasks.filter((task) => ["queued", "running", "in_progress"].includes(task.status));

  return (
    <Stack spacing={1.5} className="inspector-panel">
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction="row" justifyContent="space-between" spacing={1} alignItems="flex-start">
          <Box>
            <Typography variant="subtitle2" color="text.secondary">
              Выбранная сессия
            </Typography>
            <Typography fontWeight={800}>{session.title || "Без названия"}</Typography>
            <Typography variant="caption" color="text.secondary" className="mono">
              {session.id}
            </Typography>
          </Box>
          <Button variant="outlined" onClick={onRefresh}>
            Refresh
          </Button>
        </Stack>
        <Divider sx={{ my: 1.25 }} />
        <KeyValueTable
          rows={[
            ["Агент", `${session.agent_name} (${session.agent_profile_id})`],
            ["Модель", session.model || "—"],
            ["Сообщения", session.message_count],
            ["Контекст", session.context_tokens],
            ["Compact", session.compactifications],
            ["Auto approve", session.auto_approve ? "да" : "нет"],
            ["Обновлена", formatTime(session.updated_at)]
          ]}
        />
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Оперативное состояние
        </Typography>
        <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
          <Chip label={`tasks: ${tasks.length}`} variant="outlined" />
          <Chip label={`active: ${activeTasks.length}`} color={activeTasks.length > 0 ? "warning" : "default"} variant="outlined" />
          <Chip label={`tools: ${sessionTools.length}`} variant="outlined" />
          <Chip label={`tool errors: ${failedTools.length}`} color={failedTools.length > 0 ? "error" : "default"} variant="outlined" />
          {session.has_pending_approval ? <Chip label="approval pending" color="warning" /> : null}
        </Stack>
        <Stack direction="row" spacing={1} sx={{ mt: 1.25 }}>
          <Button color="warning" variant="outlined" onClick={onCancelRun}>
            Stop run
          </Button>
          <Button color="error" variant="outlined" onClick={onCancelAll}>
            Cancel all
          </Button>
        </Stack>
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Выбранное событие
        </Typography>
        {selectedEvent ? (
          <Stack spacing={1}>
            <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap>
              <Chip label={selectedEvent.kind} color={eventTone(selectedEvent.kind)} variant="outlined" />
              <Typography variant="caption" color="text.secondary">
                {formatTime(selectedEvent.createdAt)}
              </Typography>
            </Stack>
            <Typography fontWeight={700}>{selectedEvent.label}</Typography>
            <Typography variant="body2" color="text.secondary">
              {selectedEvent.detailTitle}
            </Typography>
            <Typography variant="caption" color="text.secondary" className="mono">
              {selectedEvent.id}
            </Typography>
            <Typography component="pre" className="inspector-detail">
              {selectedEvent.detail}
            </Typography>
          </Stack>
        ) : (
          <Typography variant="body2" color="text.secondary">
            Выбери событие в timeline.
          </Typography>
        )}
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Active run raw
        </Typography>
        <JsonBlock value={run ?? "Нет активного run."} />
      </Paper>
    </Stack>
  );
}
