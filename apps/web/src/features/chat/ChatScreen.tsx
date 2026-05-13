import { useEffect, useMemo, useRef, useState } from "react";
import { Alert, Box, Button, Chip, Divider, LinearProgress, Paper, Stack, TextField, Typography } from "@mui/material";
import { EmptyState } from "../../components/common";
import { MarkdownMessage } from "../../components/MarkdownMessage";
import type {
  PendingApproval,
  PendingChatMessage,
  SessionDebug,
  SessionPreferencesPatch,
  SessionSummary,
  SessionTask,
  SessionTranscript,
  ToolCallSummary,
  TranscriptLine
} from "../../types";
import { formatTime, short } from "../../utils/format";
import { SessionsTable } from "../sessions/SessionsTable";
import { ChatMessageToolStats } from "./ChatMessageToolStats";
import { ChatStatusPanel } from "./ChatStatusPanel";
import { ChatWorkStatus } from "./ChatWorkStatus";
import { chatCommands, filterChatCommands, type ChatCommand } from "./chatCommands";
import { SessionFilesDialog } from "./SessionFilesDialog";
import { buildToolStats, isLowSignalChatLine } from "./toolStats";

type VisibleMessage = TranscriptLine & {
  pending_id?: string;
  pending_status?: PendingChatMessage["status"];
  pending_error?: string | null;
};

export function ChatScreen({
  sessions,
  selectedSession,
  selectedSessionId,
  transcript,
  debug,
  tasks,
  tools,
  run,
  pendingMessages,
  pendingApprovals,
  sessionFilter,
  sessionsTotal,
  sessionsOffset,
  sessionsLimit,
  message,
  loading,
  detailLoading,
  detailError,
  sending,
  approving,
  liveConnected,
  onRefresh,
  onCreateSession,
  onSelectSession,
  onFilterChange,
  onSessionsPageChange,
  onMessageChange,
  onSend,
  onCommand,
  onApprove,
  onUpdateSessionPreferences,
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
  pendingMessages: PendingChatMessage[];
  pendingApprovals: PendingApproval[];
  sessionFilter: string;
  sessionsTotal: number;
  sessionsOffset: number;
  sessionsLimit: number;
  message: string;
  loading: boolean;
  detailLoading: boolean;
  detailError: string | null;
  sending: boolean;
  approving: boolean;
  liveConnected: boolean;
  onRefresh: () => void;
  onCreateSession: () => void;
  onSelectSession: (id: string) => void;
  onFilterChange: (value: string) => void;
  onSessionsPageChange: (offset: number) => void;
  onMessageChange: (value: string) => void;
  onSend: () => void;
  onCommand: (command: ChatCommand, rawInput: string) => void;
  onApprove: (approvalId?: string) => void;
  onUpdateSessionPreferences: (patch: SessionPreferencesPatch) => void;
  onCancelRun: () => void;
  onCancelAll: () => void;
}) {
  const [sessionsOpen, setSessionsOpen] = useState(false);
  const [statusOpen, setStatusOpen] = useState(false);
  const [filesOpen, setFilesOpen] = useState(false);
  const [selectedToolId, setSelectedToolId] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement | null>(null);
  const visibleMessages = useMemo<VisibleMessage[]>(() => {
    const transcriptMessages = (transcript?.entries ?? []).filter((entry) => !isLowSignalChatLine(entry));
    const selectedPendingMessages = pendingMessages
      .filter((entry) => entry.session_id === selectedSessionId)
      .filter((entry) => {
        return !transcriptMessages.some((transcriptEntry) => {
          return (
            transcriptEntry.role === "user" &&
            transcriptEntry.content === entry.content &&
            transcriptEntry.created_at >= entry.created_at - 10
          );
        });
      })
      .map((entry) => ({
        role: entry.role,
        content: entry.content,
        created_at: entry.created_at,
        pending_id: entry.id,
        pending_status: entry.status,
        pending_error: entry.error ?? null
      }));
    return [...transcriptMessages, ...selectedPendingMessages];
  }, [pendingMessages, selectedSessionId, transcript]);
  const selectedSessionTools = selectedSession ? tools.filter((tool) => tool.session_id === selectedSession.id) : [];
  const selectedToolDetails =
    selectedToolId && debug
      ? debug.entries.find((entry) => entry.kind === "tool_call" && entry.id === selectedToolId) ?? null
      : null;
  const commands = filterChatCommands(message);
  const scrollKey = `${visibleMessages.length}:${visibleMessages.at(-1)?.created_at ?? 0}:${visibleMessages.at(-1)?.content.length ?? 0}:${sending}:${pendingApprovals.length}`;

  useEffect(() => {
    window.requestAnimationFrame(() => {
      messagesEndRef.current?.scrollIntoView({ block: "end" });
    });
  }, [scrollKey]);

  function runCommand(command: ChatCommand, rawInput = message) {
    switch (command.id) {
      case "new-session":
        onMessageChange("");
        onCreateSession();
        break;
      case "refresh":
        onMessageChange("");
        onRefresh();
        break;
      case "open-sessions":
        onMessageChange("");
        setSessionsOpen(true);
        break;
      case "open-status":
        onMessageChange("");
        setStatusOpen(true);
        break;
      case "open-files":
        onMessageChange("");
        setFilesOpen(true);
        break;
      case "model":
      case "think":
      case "rename":
        if (rawInput.trim() === command.command) {
          onMessageChange(`${command.command} `);
          break;
        }
        onCommand(command, rawInput);
        break;
      case "approve":
      case "autoapprove":
      case "compact":
      case "open-tasks":
      case "open-tools":
      case "open-skills":
      case "open-debug":
      case "open-agents":
      case "open-routes":
        onMessageChange("");
        onCommand(command, rawInput);
        break;
      case "stop":
        onMessageChange("");
        onCancelRun();
        break;
      case "cancel":
        onMessageChange("");
        onCancelAll();
        break;
      case "clear-input":
        onMessageChange("");
        break;
      case "send-help":
        onMessageChange("");
        onCommand(command, rawInput);
        break;
    }
  }

  function submitComposer() {
    const trimmed = message.trim();
    const commandName = trimmed.split(/\s+/, 1)[0].toLowerCase();
    const exactCommand = chatCommands.find((command) => command.command === commandName);
    if (exactCommand) {
      runCommand(exactCommand, trimmed);
      return;
    }
    onSend();
  }

  return (
    <Box className={`chat-layout ${sessionsOpen ? "chat-layout-left-open" : ""} ${statusOpen ? "chat-layout-right-open" : ""}`}>
      <Box className="chat-side-panel chat-side-panel-left">
        {sessionsOpen ? (
          <Stack spacing={1.25}>
            <Stack direction="row" spacing={1} alignItems="center" justifyContent="space-between">
              <Typography fontWeight={700}>Сессии</Typography>
              <Button variant="outlined" onClick={() => setSessionsOpen(false)}>
                Свернуть
              </Button>
            </Stack>
            <SessionsTable
              sessions={sessions}
              selectedId={selectedSessionId}
              filter={sessionFilter}
              total={sessionsTotal}
              offset={sessionsOffset}
              limit={sessionsLimit}
              onFilterChange={onFilterChange}
              onSelect={(id) => {
                onSelectSession(id);
                setSessionsOpen(false);
              }}
              onPageChange={onSessionsPageChange}
            />
          </Stack>
        ) : (
          <button className="chat-rail-button" type="button" onClick={() => setSessionsOpen(true)}>
            Сессии
          </button>
        )}
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
                <Chip
                  label={liveConnected ? "live: SSE" : "live: fallback poll"}
                  color={liveConnected ? "success" : "warning"}
                  variant="outlined"
                />
                <Button variant="outlined" onClick={() => setFilesOpen(true)}>
                  Файлы
                </Button>
                <Button variant="text" onClick={onRefresh} disabled={loading}>
                  Синхр.
                </Button>
                <Button variant="contained" onClick={onCreateSession}>
                  Новая сессия
                </Button>
              </Stack>
            </Box>
            <Divider />

            <Box className="chat-messages">
              {detailLoading ? <LinearProgress /> : null}
              {detailError ? <Alert severity="error">{detailError}</Alert> : null}
              {visibleMessages.length === 0 ? (
                <EmptyState title="Сообщений нет" detail="Напиши первое сообщение агенту. Команды открываются через /." />
              ) : (
                visibleMessages.map((entry, index) => (
                  <Box key={`${entry.created_at}-${index}`} className={`chat-message role-${entry.role}`}>
                    <Box className="chat-message-meta">
                      <Chip label={entry.role} color={entry.role === "assistant" ? "primary" : "default"} variant="outlined" />
                      {entry.pending_status ? <Chip label={entry.pending_status === "failed" ? "ошибка" : "отправляется"} color={entry.pending_status === "failed" ? "error" : "info"} variant="outlined" /> : null}
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
                      <>
                        <MarkdownMessage content={entry.content} />
                        <ChatMessageToolStats stats={buildToolStats(selectedSessionTools, entry.run_id)} />
                      </>
                    ) : (
                      <Typography component="pre" className="chat-user-text">
                        {entry.content}
                      </Typography>
                    )}
                    {entry.pending_error ? (
                      <Typography variant="caption" color="error">
                        {entry.pending_error}
                      </Typography>
                    ) : null}
                  </Box>
                ))
              )}
              <ChatWorkStatus
                selectedSession={selectedSession}
                tools={tools}
                tasks={tasks}
                pendingApprovals={pendingApprovals}
                run={run}
                sending={sending}
                approving={approving}
                onApprove={onApprove}
                onCancelRun={onCancelRun}
              />
              <div ref={messagesEndRef} />
            </Box>

            <Divider />
            <Box className="chat-composer">
              {commands.length > 0 ? (
                <Paper variant="outlined" className="chat-command-palette">
                  <Stack spacing={0.5}>
                    {commands.map((command) => (
                      <button key={command.id} className="chat-command-row" type="button" onClick={() => runCommand(command)}>
                        <span className="mono">{command.command}</span>
                        <strong>{command.title}</strong>
                        <span>{command.detail}</span>
                      </button>
                    ))}
                  </Stack>
                </Paper>
              ) : null}
              <TextField
                fullWidth
                multiline
                minRows={3}
                maxRows={9}
                label="Сообщение агенту"
                value={message}
                onChange={(event) => onMessageChange(event.target.value)}
                placeholder="Введите задачу, вопрос или / для команд..."
                onKeyDown={(event) => {
                  if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
                    submitComposer();
                  }
                }}
              />
              <Stack direction="row" justifyContent="space-between" alignItems="center" sx={{ mt: 1 }}>
                <Typography variant="caption" color="text.secondary">
                  Ctrl/⌘ + Enter — отправить · / — команды
                </Typography>
                <Button variant="contained" onClick={submitComposer} disabled={sending || !message.trim()}>
                  {sending ? "Отправка..." : "Отправить"}
                </Button>
              </Stack>
            </Box>
          </>
        ) : (
          <Box sx={{ p: 2 }}>
            <EmptyState title="Сессия не выбрана" detail="Разверни панель «Сессии» слева или создай новую." />
          </Box>
        )}
      </Paper>

      <Box className="chat-side-panel chat-side-panel-right">
        {statusOpen ? (
          <Stack spacing={1.25}>
            <Stack direction="row" spacing={1} alignItems="center" justifyContent="space-between">
              <Typography fontWeight={700}>Статус</Typography>
              <Button variant="outlined" onClick={() => setStatusOpen(false)}>
                Свернуть
              </Button>
            </Stack>
            <ChatStatusPanel
              selectedSession={selectedSession}
              tasks={tasks}
              tools={tools}
              pendingApprovals={pendingApprovals}
              selectedToolId={selectedToolId}
              toolDetails={selectedToolDetails}
              debugEntries={debug?.entries ?? []}
              onSelectTool={setSelectedToolId}
              onClearTool={() => setSelectedToolId(null)}
              onUpdateSessionPreferences={onUpdateSessionPreferences}
              onCancelRun={onCancelRun}
              onCancelAll={onCancelAll}
            />
          </Stack>
        ) : (
          <button className="chat-rail-button" type="button" onClick={() => setStatusOpen(true)}>
            Статус
          </button>
        )}
      </Box>
      <SessionFilesDialog open={filesOpen} selectedSession={selectedSession} onClose={() => setFilesOpen(false)} />
    </Box>
  );
}
