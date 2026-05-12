import {
  Button,
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
import { EmptyState } from "../../components/common";
import type { SessionSummary } from "../../types";
import { formatTime } from "../../utils/format";

export function SessionsTable({
  sessions,
  selectedId,
  filter,
  total,
  offset,
  limit,
  onFilterChange,
  onSelect,
  onPageChange
}: {
  sessions: SessionSummary[];
  selectedId: string | null;
  filter: string;
  total: number;
  offset: number;
  limit: number;
  onFilterChange: (value: string) => void;
  onSelect: (id: string) => void;
  onPageChange: (offset: number) => void;
}) {
  const normalizedFilter = filter.trim().toLowerCase();
  const filtered = sessions.filter((session) => {
    if (!normalizedFilter) {
      return true;
    }
    return [session.title, session.id, session.agent_name, session.last_message_preview ?? ""]
      .join(" ")
      .toLowerCase()
      .includes(normalizedFilter);
  });

  return (
    <Stack spacing={1.5}>
      <TextField
        label="Фильтр сессий"
        value={filter}
        onChange={(event) => onFilterChange(event.target.value)}
        placeholder="Название, id, агент, текст"
      />
      <Stack direction="row" spacing={1} alignItems="center" justifyContent="space-between">
        <Typography variant="caption" color="text.secondary">
          {total === 0 ? "0 сессий" : `${offset + 1}-${offset + filtered.length} из ${total}`}
        </Typography>
        <Stack direction="row" spacing={1}>
          <Button
            variant="outlined"
            disabled={offset === 0}
            onClick={() => onPageChange(Math.max(0, offset - limit))}
          >
            Назад
          </Button>
          <Button
            variant="outlined"
            disabled={offset + limit >= total}
            onClick={() => onPageChange(offset + limit)}
          >
            Вперёд
          </Button>
        </Stack>
      </Stack>
      <TableContainer component={Paper} variant="outlined" sx={{ maxHeight: "calc(100vh - 300px)" }}>
        <Table stickyHeader size="small">
          <TableHead>
            <TableRow>
              <TableCell>Сессия</TableCell>
              <TableCell>Агент</TableCell>
              <TableCell align="right">Сообщ.</TableCell>
              <TableCell>Обновлена</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {filtered.map((session) => (
              <TableRow
                key={session.id}
                hover
                selected={session.id === selectedId}
                onClick={() => onSelect(session.id)}
                sx={{ cursor: "pointer" }}
              >
                <TableCell>
                  <Typography fontWeight={700}>{session.title || "Без названия"}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {session.id}
                  </Typography>
                  {session.last_message_preview ? (
                    <Typography variant="body2" color="text.secondary" noWrap sx={{ maxWidth: 280 }}>
                      {session.last_message_preview}
                    </Typography>
                  ) : null}
                </TableCell>
                <TableCell>
                  <Typography>{session.agent_name}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {session.agent_profile_id}
                  </Typography>
                </TableCell>
                <TableCell align="right">{session.message_count}</TableCell>
                <TableCell>{formatTime(session.updated_at)}</TableCell>
              </TableRow>
            ))}
            {filtered.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4}>
                  <EmptyState title="Сессии не найдены" detail="Измени фильтр или создай новую сессию." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>
    </Stack>
  );
}
