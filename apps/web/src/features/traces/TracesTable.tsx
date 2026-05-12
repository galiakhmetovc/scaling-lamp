import { Paper, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { TraceLink } from "../../types";
import { formatTime, short } from "../../utils/format";

export function TracesTable({ traces }: { traces: TraceLink[] }) {
  if (traces.length === 0) {
    return <EmptyState title="Trace links нет" detail="Когда runtime создаст trace links, они появятся здесь." />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Trace</TableCell>
            <TableCell>Span</TableCell>
            <TableCell>Entity</TableCell>
            <TableCell>Surface</TableCell>
            <TableCell>Время</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {traces.map((trace) => (
            <TableRow key={`${trace.trace_id}-${trace.span_id}-${trace.entity_id}`} hover>
              <TableCell className="mono">{short(trace.trace_id, 28)}</TableCell>
              <TableCell className="mono">{short(trace.span_id, 18)}</TableCell>
              <TableCell>
                <Typography>{trace.entity_kind}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {short(trace.entity_id, 32)}
                </Typography>
              </TableCell>
              <TableCell>
                {trace.surface || "—"}
                {trace.entrypoint ? (
                  <Typography variant="caption" display="block" color="text.secondary">
                    {trace.entrypoint}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell>{formatTime(trace.created_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}
