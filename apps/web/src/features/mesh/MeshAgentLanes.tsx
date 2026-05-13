import { Box, Chip, Divider, Paper, Stack, Typography } from "@mui/material";
import { EmptyState, StatusChip } from "../../components/common";
import { formatTime, short } from "../../utils/format";
import type { AgentLane } from "./meshModel";

export function MeshAgentLanes({ lanes, onOpenSession }: { lanes: AgentLane[]; onOpenSession: (sessionId: string) => void }) {
  if (lanes.length === 0) {
    return <EmptyState title="Agent lanes пусты" detail="В snapshot нет Agent Profiles." />;
  }

  return (
    <Stack spacing={1.5}>
      {lanes.map((lane) => (
        <Paper key={lane.agent.id} variant="outlined">
          <Box sx={{ px: 1.5, py: 1 }}>
            <Stack direction={{ xs: "column", md: "row" }} spacing={1} justifyContent="space-between">
              <Stack minWidth={0}>
                <Typography fontWeight={700}>{lane.agent.name}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {lane.agent.id} · {lane.agent.template_kind} · updated={formatTime(lane.lastUpdated)}
                </Typography>
              </Stack>
              <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
                <Chip size="small" label={`sessions: ${lane.sessions.length}`} variant="outlined" />
                <Chip size="small" label={`runs: ${lane.activeRuns.length}`} color={lane.activeRuns.length ? "warning" : "default"} variant="outlined" />
                <Chip size="small" label={`tasks: ${lane.tasks.length}`} color={lane.activeTasks ? "warning" : "default"} variant="outlined" />
                <Chip size="small" label={`failed: ${lane.failedTasks}`} color={lane.failedTasks ? "error" : "default"} variant="outlined" />
              </Stack>
            </Stack>
          </Box>
          <Divider />
          <Stack spacing={1} sx={{ p: 1.5 }}>
            <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
              {lane.sessions.slice(0, 5).map((session) => (
                <Chip
                  key={session.id}
                  label={`${session.title || short(session.id, 18)} · ${session.message_count ?? 0} msg`}
                  variant="outlined"
                  onClick={() => onOpenSession(session.id)}
                />
              ))}
              {lane.sessions.length === 0 ? (
                <Typography variant="body2" color="text.secondary">
                  Нет последних сессий в snapshot.
                </Typography>
              ) : null}
            </Stack>
            <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
              {lane.tasks.slice(0, 4).map((task) => (
                <Chip key={task.id} label={`${task.kind}: ${short(task.id, 18)}`} variant="outlined" color={task.status === "failed" ? "error" : "default"} />
              ))}
            </Stack>
            {lane.tasks[0] ? (
              <Stack direction="row" spacing={1} alignItems="center">
                <StatusChip value={lane.tasks[0].status} />
                <Typography variant="caption" color="text.secondary">
                  latest task {short(lane.tasks[0].id, 32)} · {formatTime(lane.tasks[0].updated_at)}
                </Typography>
              </Stack>
            ) : null}
            <Typography variant="caption" color="text.secondary">
              Outputs: Telegram chats {lane.telegramChats.length}, delivery targets {lane.deliveryTargets.length}
            </Typography>
          </Stack>
        </Paper>
      ))}
    </Stack>
  );
}
