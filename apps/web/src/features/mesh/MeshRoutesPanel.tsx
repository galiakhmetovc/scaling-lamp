import { Chip, Paper, Stack, Table, TableBody, TableCell, TableHead, TableRow, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import type { DeliveryTarget, TelegramChat, WebEventBus } from "../../types";
import { formatTime, short } from "../../utils/format";

export function MeshRoutesPanel({
  eventBus,
  targets,
  chats
}: {
  eventBus: WebEventBus;
  targets: DeliveryTarget[];
  chats: TelegramChat[];
}) {
  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Event bus / NATS
        </Typography>
        <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap>
          <Chip label={`backend: ${eventBus.backend}`} color={eventBus.nats_configured ? "success" : "warning"} variant="outlined" />
          <Chip label={`required: ${eventBus.required ? "yes" : "no"}`} variant="outlined" />
          <Chip label={`input: ${eventBus.input_stream}`} variant="outlined" />
          <Chip label={`session: ${eventBus.session_stream}`} variant="outlined" />
          <Chip label={`task: ${eventBus.task_stream}`} variant="outlined" />
          <Chip label={`delivery: ${eventBus.delivery_stream}`} variant="outlined" />
          <Chip label={`dlq: ${eventBus.dlq_stream}`} color="warning" variant="outlined" />
        </Stack>
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Delivery outputs
        </Typography>
        {targets.length === 0 ? <EmptyState title="Delivery targets не настроены" /> : null}
        {targets.length > 0 ? (
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
            </TableBody>
          </Table>
        ) : null}
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Telegram inputs
        </Typography>
        {chats.length === 0 ? <EmptyState title="Telegram bindings не найдены" /> : null}
        {chats.length > 0 ? (
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>Chat</TableCell>
                <TableCell>Scope</TableCell>
                <TableCell>Session</TableCell>
                <TableCell>Agent</TableCell>
                <TableCell>Queue</TableCell>
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
                </TableRow>
              ))}
            </TableBody>
          </Table>
        ) : null}
      </Paper>
    </Stack>
  );
}
