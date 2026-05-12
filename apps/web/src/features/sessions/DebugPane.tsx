import { Chip, Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { DebugEntry, SessionDebug } from "../../types";
import { formatTime, short } from "../../utils/format";

export function DebugPane({ debug }: { debug: SessionDebug | null }) {
  if (!debug || debug.entries.length === 0) {
    return <EmptyState title="Debug-лента пуста" detail="Нет сообщений, tool calls или артефактов для этой сессии." />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Тип</TableCell>
            <TableCell>Событие</TableCell>
            <TableCell>Детали</TableCell>
            <TableCell>Время</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {debug.entries.map((entry: DebugEntry) => (
            <TableRow key={entry.id} hover>
              <TableCell>
                <Chip label={entry.kind} variant="outlined" />
              </TableCell>
              <TableCell>
                <Typography fontWeight={700}>{entry.label}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {short(entry.id, 28)}
                </Typography>
              </TableCell>
              <TableCell sx={{ maxWidth: 520 }}>
                <Typography variant="body2" fontWeight={700}>
                  {entry.detail_title}
                </Typography>
                <Typography component="pre" className="debug-detail">
                  {entry.detail}
                </Typography>
              </TableCell>
              <TableCell>{formatTime(entry.created_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
