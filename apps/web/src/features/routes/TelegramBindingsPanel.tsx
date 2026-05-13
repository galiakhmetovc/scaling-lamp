import { Button, Divider, Paper, Table, TableBody, TableCell, TableHead, TableRow, Typography, Box } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { AgentSummary, SessionSummary, TelegramChat } from "../../types";
import { describeTelegramChat, sessionTitle } from "../../ui/entityLabels";
import { formatTime, short } from "../../utils/format";

export function TelegramBindingsPanel({
  chats,
  sessions,
  agents,
  onOpenSession
}: {
  chats: TelegramChat[];
  sessions: SessionSummary[];
  agents: AgentSummary[];
  onOpenSession: (sessionId: string) => void;
}) {
  return (
    <Paper variant="outlined">
      <Box sx={{ px: 1.5, py: 1 }}>
        <Typography fontWeight={700}>Telegram bindings</Typography>
      </Box>
      <Divider />
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Chat</TableCell>
            <TableCell>Scope</TableCell>
            <TableCell>Сессия</TableCell>
            <TableCell>Агент</TableCell>
            <TableCell>Queue</TableCell>
            <TableCell>Обновлён</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {chats.map((chat) => {
            const label = describeTelegramChat(chat, sessions, agents);
            const linkedSession = sessionTitle(chat.selected_session_id, sessions);
            return (
              <TableRow key={chat.telegram_chat_id} hover>
                <TableCell>
                  <Typography fontWeight={700}>{label.primary}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {label.technical}
                  </Typography>
                </TableCell>
                <TableCell>{chat.scope}</TableCell>
                <TableCell>
                  {chat.selected_session_id ? (
                    <Button size="small" variant="text" onClick={() => onOpenSession(chat.selected_session_id!)} sx={{ textTransform: "none" }}>
                      {linkedSession.primary}
                    </Button>
                  ) : (
                    "—"
                  )}
                  {chat.selected_session_id ? (
                    <Typography variant="caption" color="text.secondary" className="mono" display="block">
                      {short(chat.selected_session_id, 28)}
                    </Typography>
                  ) : null}
                </TableCell>
                <TableCell>{label.secondary.replace(/^Telegram \S+ · /, "") || chat.default_agent_profile_id || "—"}</TableCell>
                <TableCell>
                  {chat.inbound_queue_mode}
                  {chat.inbound_coalesce_window_ms ? ` · ${chat.inbound_coalesce_window_ms}ms` : ""}
                </TableCell>
                <TableCell>{formatTime(chat.updated_at)}</TableCell>
              </TableRow>
            );
          })}
          {chats.length === 0 ? (
            <TableRow>
              <TableCell colSpan={6}>
                <EmptyState title="Telegram bindings не найдены" />
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </Paper>
  );
}
