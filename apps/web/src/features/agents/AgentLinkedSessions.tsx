import { Alert, Button, Paper, Stack, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { SessionSummary } from "../../types";
import { formatTime } from "../../utils/format";

export function AgentLinkedSessions({
  sessions,
  loading,
  offset,
  limit,
  hasMore,
  onPageChange,
  onOpenSession
}: {
  sessions: SessionSummary[];
  loading: boolean;
  offset: number;
  limit: number;
  hasMore: boolean;
  onPageChange: (offset: number) => void;
  onOpenSession: (sessionId: string) => void;
}) {
  return (
    <Stack spacing={1}>
      <Stack direction={{ xs: "column", sm: "row" }} spacing={1} justifyContent="space-between" alignItems={{ sm: "center" }}>
        <Stack>
          <Typography fontWeight={700}>Связанные сессии</Typography>
          <Typography variant="caption" color="text.secondary">
            Серверная выборка по текущему Agent Profile.
          </Typography>
        </Stack>
        <Stack direction="row" spacing={1}>
          <Button size="small" variant="outlined" disabled={loading || offset === 0} onClick={() => onPageChange(Math.max(0, offset - limit))}>
            Назад
          </Button>
          <Button size="small" variant="outlined" disabled={loading || !hasMore} onClick={() => onPageChange(offset + limit)}>
            Дальше
          </Button>
        </Stack>
      </Stack>
      {loading ? <Alert severity="info">Загружаю связанные сессии...</Alert> : null}
      {!loading && sessions.length === 0 ? (
        <EmptyState title="Связанных сессий нет" detail="Для этого Agent Profile пока нет сессий на выбранной странице." />
      ) : null}
      {sessions.map((session) => (
        <Paper key={session.id} variant="outlined" sx={{ p: 1.25 }}>
          <Stack direction={{ xs: "column", md: "row" }} spacing={1} justifyContent="space-between">
            <Stack minWidth={0}>
              <Typography fontWeight={700}>{session.title || session.id}</Typography>
              <Typography variant="caption" color="text.secondary" className="mono">
                {session.id} · сообщений={session.message_count} · обновлена={formatTime(session.updated_at)}
              </Typography>
            </Stack>
            <Button variant="outlined" onClick={() => onOpenSession(session.id)}>
              Открыть чат
            </Button>
          </Stack>
        </Paper>
      ))}
    </Stack>
  );
}
