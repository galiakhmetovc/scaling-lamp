import {
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Typography
} from "@mui/material";
import { EmptyState, StatusChip } from "../../components/common";
import type { ToolCallSummary } from "../../types";
import { formatTime, short } from "../../utils/format";

export function ToolsTable({
  tools,
  filter,
  onFilterChange
}: {
  tools: ToolCallSummary[];
  filter: string;
  onFilterChange: (value: string) => void;
}) {
  const normalizedFilter = filter.trim().toLowerCase();
  const filtered = tools.filter((tool) => {
    if (!normalizedFilter) {
      return true;
    }
    return [tool.tool_name, tool.status, tool.summary, tool.error ?? "", tool.result_summary ?? "", tool.session_id]
      .join(" ")
      .toLowerCase()
      .includes(normalizedFilter);
  });

  return (
    <Stack spacing={1.5}>
      <TextField
        label="Фильтр tool calls"
        value={filter}
        onChange={(event) => onFilterChange(event.target.value)}
        placeholder="tool, статус, summary, session_id"
      />
      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>Tool</TableCell>
              <TableCell>Статус</TableCell>
              <TableCell>Summary</TableCell>
              <TableCell>Результат</TableCell>
              <TableCell>Сессия</TableCell>
              <TableCell>Время</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {filtered.map((tool) => (
              <TableRow key={tool.id} hover>
                <TableCell>
                  <Typography fontWeight={700} className="mono">
                    {tool.tool_name}
                  </Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {short(tool.id, 28)}
                  </Typography>
                </TableCell>
                <TableCell>
                  <StatusChip value={tool.status} />
                  {tool.error ? (
                    <Typography variant="caption" color="error" display="block" sx={{ mt: 0.5 }}>
                      {tool.error}
                    </Typography>
                  ) : null}
                </TableCell>
                <TableCell sx={{ maxWidth: 420 }}>{tool.summary}</TableCell>
                <TableCell sx={{ maxWidth: 300 }}>
                  {tool.result_summary || "—"}
                  {tool.result_artifact_id ? (
                    <Typography variant="caption" color="text.secondary" display="block" className="mono">
                      artifact: {short(tool.result_artifact_id, 24)}
                    </Typography>
                  ) : null}
                </TableCell>
                <TableCell className="mono">{short(tool.session_id, 24)}</TableCell>
                <TableCell>{formatTime(tool.updated_at)}</TableCell>
              </TableRow>
            ))}
            {filtered.length === 0 ? (
              <TableRow>
                <TableCell colSpan={6}>
                  <EmptyState title="Tool calls не найдены" detail="Нет данных под текущий фильтр." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>
    </Stack>
  );
}
