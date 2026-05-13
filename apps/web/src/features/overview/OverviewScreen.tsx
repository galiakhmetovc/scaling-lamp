import { Box, Button, Chip, Paper, Stack, Table, TableBody, TableCell, TableRow, Typography } from "@mui/material";
import { EmptyState, Metric, SectionHeader } from "../../components/common";
import type { SessionSummary, WebSnapshot } from "../../types";
import { formatTime, short } from "../../utils/format";
import { RunsTable } from "../runs/RunsTable";

export function OverviewScreen({
  snapshot,
  sessions,
  loading,
  toolErrors,
  activeRuns,
  onRefresh,
  onOpenSession
}: {
  snapshot: WebSnapshot | null;
  sessions: SessionSummary[];
  loading: boolean;
  toolErrors: number;
  activeRuns: number;
  onRefresh: () => void;
  onOpenSession: (sessionId: string) => void;
}) {
  if (!snapshot) {
    return <EmptyState title="Snapshot недоступен" detail="Нет данных от /v1/web/snapshot." />;
  }
  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Обзор runtime"
        subtitle="Read-only состояние agentd, NATS, Postgres и последних операций."
        action={
          <Button variant="outlined" onClick={onRefresh} disabled={loading}>
            Обновить
          </Button>
        }
      />
      <Stack direction="row" spacing={1.5} flexWrap="wrap" useFlexGap>
        <Metric label="Сессии" value={snapshot.status.session_count} hint={`${sessions.length} в списке`} />
        <Metric label="Runs" value={snapshot.status.run_count} hint={`${activeRuns} активных`} />
        <Metric label="Jobs" value={snapshot.status.job_count} />
        <Metric label="Tool errors" value={toolErrors} hint={`${snapshot.recent_tool_calls.length} последних вызовов`} />
        <Metric label="Agents" value={snapshot.agents.length} />
        <Metric label="DB" value={snapshot.status.database || "—"} />
      </Stack>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
          <Chip label={`version: ${snapshot.status.version ?? "unknown"}`} variant="outlined" />
          <Chip label={`commit: ${short(snapshot.status.commit, 12)}`} variant="outlined" />
          <Chip
            label={`tree: ${snapshot.status.tree_state ?? "unknown"}`}
            color={snapshot.status.tree_state === "clean" ? "success" : "warning"}
            variant="outlined"
          />
          <Chip label={`permission: ${snapshot.status.permission_mode}`} variant="outlined" />
          <Chip
            label={`event bus: ${snapshot.event_bus.backend}`}
            color={snapshot.event_bus.nats_configured ? "success" : "warning"}
            variant="outlined"
          />
          <Chip label={`generated: ${formatTime(snapshot.generated_at)}`} variant="outlined" />
        </Stack>
      </Paper>

      <Box className="grid-two">
        <Paper variant="outlined" sx={{ p: 1.5 }}>
          <Typography fontWeight={700} sx={{ mb: 1 }}>
            Event bus
          </Typography>
          <Table size="small">
            <TableBody>
              <TableRow>
                <TableCell>Backend</TableCell>
                <TableCell>{snapshot.event_bus.backend}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell>Required</TableCell>
                <TableCell>{snapshot.event_bus.required ? "да" : "нет"}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell>NATS</TableCell>
                <TableCell>{snapshot.event_bus.nats_configured ? "настроен" : "не настроен"}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell>Streams</TableCell>
                <TableCell className="mono">
                  {[
                    snapshot.event_bus.input_stream,
                    snapshot.event_bus.session_stream,
                    snapshot.event_bus.delivery_stream,
                    snapshot.event_bus.task_stream,
                    snapshot.event_bus.dlq_stream
                  ].join(" · ")}
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Paper>
        <Paper variant="outlined" sx={{ p: 1.5 }}>
          <Typography fontWeight={700} sx={{ mb: 1 }}>
            Runtime paths
          </Typography>
          <Table size="small">
            <TableBody>
              <TableRow>
                <TableCell>Data dir</TableCell>
                <TableCell className="mono">{snapshot.status.data_dir}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell>Build</TableCell>
                <TableCell className="mono">{snapshot.status.build_id || "—"}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell>Traces</TableCell>
                <TableCell>{snapshot.recent_traces.length}</TableCell>
              </TableRow>
              <TableRow>
                <TableCell>Telegram chats</TableCell>
                <TableCell>{snapshot.telegram_chats.length}</TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Paper>
      </Box>

      <RunsTable runs={snapshot.recent_runs} sessions={sessions} onOpenSession={onOpenSession} />
    </Stack>
  );
}
