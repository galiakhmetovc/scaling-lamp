import { Button, Paper, Stack, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { SessionSummary } from "../../types";
import { formatTime } from "../../utils/format";

export function AgentLinkedSessions({
  sessions,
  onOpenSession
}: {
  sessions: SessionSummary[];
  onOpenSession: (sessionId: string) => void;
}) {
  if (sessions.length === 0) {
    return <EmptyState title="Связанных сессий нет" detail="На текущей странице списка сессий нет сессий этого агента." />;
  }

  return (
    <Stack spacing={1}>
      {sessions.slice(0, 8).map((session) => (
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
