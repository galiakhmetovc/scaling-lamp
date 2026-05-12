import { Box, Divider, Paper, Stack, Table, TableBody, TableCell, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { DeliveryTarget, TelegramChat } from "../../types";
import { formatTime, short } from "../../utils/format";

export function RoutesView({ targets, chats }: { targets: DeliveryTarget[]; chats: TelegramChat[] }) {
  return (
    <Stack spacing={2}>
      <Paper variant="outlined">
        <Box sx={{ px: 1.5, py: 1 }}>
          <Typography fontWeight={700}>Delivery targets</Typography>
        </Box>
        <Divider />
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>Target</TableCell>
              <TableCell>Kind</TableCell>
              <TableCell>Scope</TableCell>
              <TableCell>Format</TableCell>
              <TableCell>Обновлён</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {targets.map((target) => (
              <TableRow key={target.target_id} hover>
                <TableCell className="mono">{target.target_id}</TableCell>
                <TableCell>{target.kind}</TableCell>
                <TableCell>{target.scope}</TableCell>
                <TableCell>{target.format_policy}</TableCell>
                <TableCell>{formatTime(target.updated_at)}</TableCell>
              </TableRow>
            ))}
            {targets.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5}>
                  <EmptyState title="Delivery targets не настроены" />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </Paper>

      <Paper variant="outlined">
        <Box sx={{ px: 1.5, py: 1 }}>
          <Typography fontWeight={700}>Telegram bindings</Typography>
        </Box>
        <Divider />
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>Chat ID</TableCell>
              <TableCell>Scope</TableCell>
              <TableCell>Сессия</TableCell>
              <TableCell>Агент</TableCell>
              <TableCell>Queue</TableCell>
              <TableCell>Обновлён</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {chats.map((chat) => (
              <TableRow key={chat.telegram_chat_id} hover>
                <TableCell className="mono">{chat.telegram_chat_id}</TableCell>
                <TableCell>{chat.scope}</TableCell>
                <TableCell className="mono">{short(chat.selected_session_id, 28)}</TableCell>
                <TableCell>{chat.default_agent_profile_id || "—"}</TableCell>
                <TableCell>
                  {chat.inbound_queue_mode}
                  {chat.inbound_coalesce_window_ms ? ` · ${chat.inbound_coalesce_window_ms}ms` : ""}
                </TableCell>
                <TableCell>{formatTime(chat.updated_at)}</TableCell>
              </TableRow>
            ))}
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
    </Stack>
  );
}
