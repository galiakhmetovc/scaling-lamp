import { Alert, Box, Button, Chip, LinearProgress, Paper, Stack, Typography } from "@mui/material";
import { StatusChip } from "../../components/common";
import type { PendingApproval, SessionSummary, SessionTask, ToolCallSummary } from "../../types";
import { formatTime, short } from "../../utils/format";
import { deriveWorkStatus } from "./chatStatus";

export function ChatWorkStatus({
  selectedSession,
  tools,
  tasks,
  pendingApprovals,
  run,
  sending,
  approving,
  onApprove,
  onCancelRun
}: {
  selectedSession: SessionSummary | null;
  tools: ToolCallSummary[];
  tasks: SessionTask[];
  pendingApprovals: PendingApproval[];
  run: unknown;
  sending: boolean;
  approving: boolean;
  onApprove: (approvalId?: string) => void;
  onCancelRun: () => void;
}) {
  const status = deriveWorkStatus({ selectedSession, tools, tasks, pendingApprovals, run, sending });

  if (!status) {
    return null;
  }

  return (
    <Paper variant="outlined" className={`chat-work-status chat-work-status-${status.severity}`}>
      {status.active ? <LinearProgress color={status.severity === "error" ? "error" : "primary"} /> : null}
      <Box sx={{ p: 1.25 }}>
        <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap>
          <Chip label={status.title} color={status.severity === "warning" ? "warning" : "primary"} size="small" />
          <Chip label={`Вызовы: ${status.toolCount}`} size="small" variant="outlined" />
          <Chip
            label={`Ошибки: ${status.errorCount}`}
            color={status.errorCount > 0 ? "error" : "default"}
            size="small"
            variant="outlined"
          />
          <Chip label={`Задачи: ${status.activeTaskCount}`} size="small" variant="outlined" />
          {status.latestTool ? <StatusChip value={status.latestTool.status} /> : null}
        </Stack>
        <Typography variant="body2" sx={{ mt: 1 }} color={status.severity === "error" ? "error" : "text.primary"}>
          {status.detail}
        </Typography>
        {status.latestTool ? (
          <Typography variant="caption" color="text.secondary" className="mono">
            {status.latestTool.tool_name} · старт: {formatTime(status.latestTool.requested_at)} · обновлено:{" "}
            {formatTime(status.latestTool.updated_at)} · {short(status.latestTool.run_id, 20)}
          </Typography>
        ) : null}
        {status.latestApproval ? (
          <Alert
            severity="warning"
            sx={{ mt: 1 }}
            action={
              <Stack direction="row" spacing={1}>
                <Button
                  size="small"
                  color="warning"
                  variant="contained"
                  disabled={approving}
                  onClick={() => onApprove(status.latestApproval?.approval_id)}
                >
                  Approve
                </Button>
                <Button size="small" color="error" variant="outlined" disabled={approving} onClick={onCancelRun}>
                  Stop
                </Button>
              </Stack>
            }
          >
            {status.latestApproval.approval_id}
          </Alert>
        ) : null}
      </Box>
    </Paper>
  );
}
