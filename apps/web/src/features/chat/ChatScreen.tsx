import { Alert, Box, Button, Chip, Divider, LinearProgress, Paper, Stack, TextField, Typography } from "@mui/material";
import { EmptyState, KeyValueTable, SectionHeader, StatusChip } from "../../components/common";
import { MarkdownMessage } from "../../components/MarkdownMessage";
import type { SessionSummary, SessionTask, SessionTranscript, ToolCallSummary } from "../../types";
import { formatTime, short } from "../../utils/format";
import { SessionsTable } from "../sessions/SessionsTable";

export function ChatScreen({
  sessions,
  selectedSession,
  selectedSessionId,
  transcript,
  tasks,
  tools,
  sessionFilter,
  message,
  loading,
  detailLoading,
  detailError,
  sending,
  onRefresh,
  onCreateSession,
  onSelectSession,
  onFilterChange,
  onMessageChange,
  onSend,
  onCancelRun,
  onCancelAll
}: {
  sessions: SessionSummary[];
  selectedSession: SessionSummary | null;
  selectedSessionId: string | null;
  transcript: SessionTranscript | null;
  tasks: SessionTask[];
  tools: ToolCallSummary[];
  sessionFilter: string;
  message: string;
  loading: boolean;
  detailLoading: boolean;
  detailError: string | null;
  sending: boolean;
  onRefresh: () => void;
  onCreateSession: () => void;
  onSelectSession: (id: string) => void;
  onFilterChange: (value: string) => void;
  onMessageChange: (value: string) => void;
  onSend: () => void;
  onCancelRun: () => void;
  onCancelAll: () => void;
}) {
  const visibleMessages = (transcript?.entries ?? []).filter((entry) => !entry.tool_name);
  const activeTasks = tasks.filter((task) => ["queued", "running", "in_progress"].includes(task.status));
  const selectedSessionTools = selectedSession ? tools.filter((tool) => tool.session_id === selectedSession.id) : [];
  const selectedSessionToolErrors = selectedSessionTools.filter((tool) => tool.status !== "completed" || tool.error);

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Чат"
        subtitle="Основной рабочий экран. Сообщение отправляется в выбранную сессию через canonical /v1/chat/turn."
        action={
          <Stack direction="row" spacing={1}>
            <Button variant="outlined" onClick={onRefresh} disabled={loading}>
              Обновить
            </Button>
            <Button variant="contained" onClick={onCreateSession}>
              Новая сессия
            </Button>
          </Stack>
        }
      />

      <Box className="chat-layout">
        <Box className="chat-session-list">
          <SessionsTable
            sessions={sessions}
            selectedId={selectedSessionId}
            filter={sessionFilter}
            onFilterChange={onFilterChange}
            onSelect={onSelectSession}
          />
        </Box>

        <Paper variant="outlined" className="chat-main">
          {selectedSession ? (
            <>
              <Box className="chat-header">
                <Box>
                  <Typography variant="h6">{selectedSession.title || selectedSession.id}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {selectedSession.id}
                  </Typography>
                </Box>
                <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap justifyContent="flex-end">
                  <Chip label={selectedSession.agent_name} color="primary" variant="outlined" />
                  <Chip label={selectedSession.model || "model: —"} variant="outlined" />
                  <Chip label={`tools: ${selectedSessionTools.length}`} variant="outlined" />
                  <Chip
                    label={`errors: ${selectedSessionToolErrors.length}`}
                    color={selectedSessionToolErrors.length ? "error" : "default"}
                    variant="outlined"
                  />
                </Stack>
              </Box>
              <Divider />

              <Box className="chat-messages">
                {detailLoading ? <LinearProgress /> : null}
                {detailError ? <Alert severity="error">{detailError}</Alert> : null}
                {visibleMessages.length === 0 ? (
                  <EmptyState title="Сообщений нет" detail="Напиши первое сообщение агенту." />
                ) : (
                  visibleMessages.map((entry, index) => (
                    <Box key={`${entry.created_at}-${index}`} className={`chat-message role-${entry.role}`}>
                      <Box className="chat-message-meta">
                        <Chip label={entry.role} color={entry.role === "assistant" ? "primary" : "default"} variant="outlined" />
                        <Typography variant="caption" color="text.secondary">
                          {formatTime(entry.created_at)}
                        </Typography>
                        {entry.run_id ? (
                          <Typography variant="caption" color="text.secondary" className="mono">
                            {short(entry.run_id, 20)}
                          </Typography>
                        ) : null}
                      </Box>
                      {entry.role === "assistant" ? (
                        <MarkdownMessage content={entry.content} />
                      ) : (
                        <Typography component="pre" className="chat-user-text">
                          {entry.content}
                        </Typography>
                      )}
                    </Box>
                  ))
                )}
              </Box>

              <Divider />
              <Box className="chat-composer">
                <TextField
                  fullWidth
                  multiline
                  minRows={3}
                  maxRows={9}
                  label="Сообщение агенту"
                  value={message}
                  onChange={(event) => onMessageChange(event.target.value)}
                  placeholder="Введите задачу, вопрос или команду..."
                  onKeyDown={(event) => {
                    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
                      onSend();
                    }
                  }}
                />
                <Stack direction="row" justifyContent="space-between" alignItems="center" sx={{ mt: 1 }}>
                  <Typography variant="caption" color="text.secondary">
                    Ctrl/⌘ + Enter — отправить
                  </Typography>
                  <Button variant="contained" onClick={onSend} disabled={sending || !message.trim()}>
                    {sending ? "Отправка..." : "Отправить"}
                  </Button>
                </Stack>
              </Box>
            </>
          ) : (
            <Box sx={{ p: 2 }}>
              <EmptyState title="Сессия не выбрана" detail="Выбери сессию слева или создай новую." />
            </Box>
          )}
        </Paper>

        <Box className="chat-inspector">
          <Paper variant="outlined" sx={{ p: 1.5 }}>
            <Typography fontWeight={700} sx={{ mb: 1 }}>
              Статус
            </Typography>
            {selectedSession ? (
              <Stack spacing={1.25}>
                <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
                  <Chip label={`сообщений: ${selectedSession.message_count}`} variant="outlined" />
                  <Chip label={`context: ${selectedSession.context_tokens}`} variant="outlined" />
                  <Chip label={`compact: ${selectedSession.compactifications}`} variant="outlined" />
                  <Chip label={`tasks: ${tasks.length}`} variant="outlined" />
                  <Chip label={`active: ${activeTasks.length}`} color={activeTasks.length ? "warning" : "default"} variant="outlined" />
                  {selectedSession.has_pending_approval ? <Chip label="approval pending" color="warning" /> : null}
                </Stack>
                <KeyValueTable
                  rows={[
                    ["Агент", `${selectedSession.agent_name} (${selectedSession.agent_profile_id})`],
                    ["Модель", selectedSession.model || "—"],
                    ["Auto approve", selectedSession.auto_approve ? "да" : "нет"],
                    ["Обновлена", formatTime(selectedSession.updated_at)]
                  ]}
                />
                <Stack direction="row" spacing={1}>
                  <Button color="warning" variant="outlined" onClick={onCancelRun}>
                    Stop run
                  </Button>
                  <Button color="error" variant="outlined" onClick={onCancelAll}>
                    Cancel all
                  </Button>
                </Stack>
              </Stack>
            ) : (
              <Typography variant="body2" color="text.secondary">
                Нет выбранной сессии.
              </Typography>
            )}
          </Paper>

          <Paper variant="outlined" sx={{ p: 1.5 }}>
            <Typography fontWeight={700} sx={{ mb: 1 }}>
              Последние tools
            </Typography>
            {selectedSessionTools.length === 0 ? (
              <Typography variant="body2" color="text.secondary">
                Нет tool calls в snapshot.
              </Typography>
            ) : (
              <Stack spacing={1}>
                {selectedSessionTools.slice(0, 8).map((tool) => (
                  <Box key={tool.id} className="chat-tool-row">
                    <Stack direction="row" justifyContent="space-between" spacing={1}>
                      <Typography className="mono" fontWeight={700}>
                        {tool.tool_name}
                      </Typography>
                      <StatusChip value={tool.status} />
                    </Stack>
                    <Typography variant="caption" color={tool.error ? "error" : "text.secondary"}>
                      {tool.error || tool.summary}
                    </Typography>
                  </Box>
                ))}
              </Stack>
            )}
          </Paper>
        </Box>
      </Box>
    </Stack>
  );
}
