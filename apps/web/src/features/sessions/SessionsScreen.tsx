import { Alert, Box, Button, Chip, LinearProgress, Paper, Stack, Tab, Tabs, TextField, Typography } from "@mui/material";
import { EmptyState, JsonBlock, SectionHeader } from "../../components/common";
import type { SessionDebug, SessionSummary, SessionTask, SessionTranscript, ToolCallSummary } from "../../types";
import { formatTime } from "../../utils/format";
import { DebugPane } from "./DebugPane";
import { SessionInspector } from "./SessionInspector";
import { SessionsTable } from "./SessionsTable";
import { SessionTimeline } from "./SessionTimeline";
import { TasksPane } from "./TasksPane";
import { TranscriptPane } from "./TranscriptPane";
import type { SessionEvent, SessionPane } from "./sessionEvents";

export function SessionsScreen({
  sessions,
  selectedSession,
  selectedSessionId,
  transcript,
  debug,
  tasks,
  tools,
  run,
  sessionPane,
  sessionEvents,
  selectedEvent,
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
  onPaneChange,
  onSelectEvent,
  onMessageChange,
  onSend,
  onCancelRun,
  onCancelAll
}: {
  sessions: SessionSummary[];
  selectedSession: SessionSummary | null;
  selectedSessionId: string | null;
  transcript: SessionTranscript | null;
  debug: SessionDebug | null;
  tasks: SessionTask[];
  tools: ToolCallSummary[];
  run: unknown;
  sessionPane: SessionPane;
  sessionEvents: SessionEvent[];
  selectedEvent: SessionEvent | null;
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
  onPaneChange: (value: SessionPane) => void;
  onSelectEvent: (id: string) => void;
  onMessageChange: (value: string) => void;
  onSend: () => void;
  onCancelRun: () => void;
  onCancelAll: () => void;
}) {
  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Сессии"
        subtitle="Операторский экран: список → timeline → inspector. Сообщения идут только через canonical /v1/chat/turn."
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
      <Box className="session-layout">
        <Box className="session-list">
          <SessionsTable
            sessions={sessions}
            selectedId={selectedSessionId}
            filter={sessionFilter}
            onFilterChange={onFilterChange}
            onSelect={onSelectSession}
          />
        </Box>
        <Box className="session-workspace">
          {selectedSession ? (
            <Stack spacing={1.5}>
              <Paper variant="outlined" sx={{ p: 1.5 }}>
                <Stack direction="row" justifyContent="space-between" alignItems="flex-start" spacing={2}>
                  <Box>
                    <Typography variant="h6">{selectedSession.title || selectedSession.id}</Typography>
                    <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap sx={{ mt: 1 }}>
                      <Chip label={selectedSession.agent_name} color="primary" variant="outlined" />
                      <Chip label={`model: ${selectedSession.model || "—"}`} variant="outlined" />
                      <Chip label={`context: ${selectedSession.context_tokens}`} variant="outlined" />
                      <Chip label={`compact: ${selectedSession.compactifications}`} variant="outlined" />
                      {selectedSession.has_pending_approval ? <Chip label="approval pending" color="warning" /> : null}
                    </Stack>
                  </Box>
                  <Typography variant="caption" color="text.secondary" textAlign="right">
                    Обновлена
                    <br />
                    {formatTime(selectedSession.updated_at)}
                  </Typography>
                </Stack>
              </Paper>

              <Paper variant="outlined" sx={{ p: 1 }}>
                <Stack direction={{ xs: "column", md: "row" }} spacing={1} alignItems="stretch">
                  <TextField
                    fullWidth
                    multiline
                    minRows={2}
                    label="Сообщение агенту"
                    value={message}
                    onChange={(event) => onMessageChange(event.target.value)}
                    placeholder="Введите команду или вопрос..."
                    onKeyDown={(event) => {
                      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
                        onSend();
                      }
                    }}
                  />
                  <Button variant="contained" onClick={onSend} disabled={sending || !message.trim()}>
                    {sending ? "Отправка..." : "Отправить"}
                  </Button>
                </Stack>
              </Paper>

              <Paper variant="outlined">
                <Tabs value={sessionPane} onChange={(_, value: SessionPane) => onPaneChange(value)} variant="scrollable" scrollButtons="auto">
                  <Tab value="timeline" label={`Timeline (${sessionEvents.length})`} />
                  <Tab value="transcript" label="Transcript" />
                  <Tab value="debug" label="Debug" />
                  <Tab value="tasks" label={`Tasks (${tasks.length})`} />
                  <Tab value="run" label="Active run" />
                </Tabs>
              </Paper>

              {detailLoading ? <LinearProgress /> : null}
              {detailError ? <Alert severity="error">{detailError}</Alert> : null}
              {sessionPane === "timeline" ? (
                <SessionTimeline events={sessionEvents} selectedEventId={selectedEvent?.id ?? null} onSelectEvent={onSelectEvent} />
              ) : null}
              {sessionPane === "transcript" ? <TranscriptPane transcript={transcript} /> : null}
              {sessionPane === "debug" ? <DebugPane debug={debug} /> : null}
              {sessionPane === "tasks" ? <TasksPane tasks={tasks} /> : null}
              {sessionPane === "run" ? <JsonBlock value={run ?? "Нет активного run."} /> : null}
            </Stack>
          ) : (
            <EmptyState title="Сессия не выбрана" detail="Выбери сессию слева или создай новую." />
          )}
        </Box>
        <Box className="session-inspector">
          <SessionInspector
            session={selectedSession}
            selectedEvent={selectedEvent}
            tasks={tasks}
            tools={tools}
            run={run}
            onRefresh={onRefresh}
            onCancelRun={onCancelRun}
            onCancelAll={onCancelAll}
          />
        </Box>
      </Box>
    </Stack>
  );
}
