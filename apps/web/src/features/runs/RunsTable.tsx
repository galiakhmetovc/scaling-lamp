import { Button, Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState, StatusChip } from "../../components/common";
import type { RunSummary, SessionSummary } from "../../types";
import { formatTime, short } from "../../utils/format";
import { describeRun } from "../../ui/entityLabels";

export function RunsTable({
  runs,
  sessions = [],
  onOpenSession
}: {
  runs: RunSummary[];
  sessions?: SessionSummary[];
  onOpenSession?: (sessionId: string) => void;
}) {
  if (runs.length === 0) {
    return <EmptyState title="Run history пуст" />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Run</TableCell>
            <TableCell>Сессия</TableCell>
            <TableCell>Статус</TableCell>
            <TableCell>Начат</TableCell>
            <TableCell>Обновлён</TableCell>
          </TableRow>
        </TableHead>
          <TableBody>
          {runs.map((run) => {
            const label = describeRun(run, sessions);
            return (
              <TableRow key={run.id} hover>
                <TableCell>
                  <Typography fontWeight={700}>{label.primary}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {short(label.technical, 42)}
                  </Typography>
                </TableCell>
                <TableCell>
                  {onOpenSession ? (
                    <Button size="small" variant="text" onClick={() => onOpenSession(run.session_id)} sx={{ textTransform: "none" }}>
                      {label.secondary}
                    </Button>
                  ) : (
                    <>
                      <Typography variant="body2">{label.secondary}</Typography>
                      <Typography variant="caption" color="text.secondary" className="mono">
                        {short(run.session_id, 28)}
                      </Typography>
                    </>
                  )}
                </TableCell>
                <TableCell>
                  <StatusChip value={run.status} />
                  {run.error ? (
                    <Typography variant="caption" color="error" display="block" sx={{ mt: 0.5 }}>
                      {run.error}
                    </Typography>
                  ) : null}
                </TableCell>
                <TableCell>{formatTime(run.started_at)}</TableCell>
                <TableCell>{formatTime(run.updated_at)}</TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
