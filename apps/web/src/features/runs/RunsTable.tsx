import { Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState, StatusChip } from "../../components/common";
import type { RunSummary } from "../../types";
import { formatTime, short } from "../../utils/format";

export function RunsTable({ runs }: { runs: RunSummary[] }) {
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
          {runs.map((run) => (
            <TableRow key={run.id} hover>
              <TableCell className="mono">{short(run.id, 32)}</TableCell>
              <TableCell className="mono">{short(run.session_id, 28)}</TableCell>
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
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
