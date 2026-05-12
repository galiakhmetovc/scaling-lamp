import { useEffect, useState, type ReactNode } from "react";
import {
  Alert,
  AppBar,
  Box,
  Button,
  Chip,
  CircularProgress,
  CssBaseline,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Divider,
  FormControl,
  InputLabel,
  LinearProgress,
  List,
  ListItemButton,
  ListItemText,
  MenuItem,
  Paper,
  Select,
  Stack,
  Tab,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Tabs,
  TextField,
  ThemeProvider,
  Toolbar,
  Typography,
  createTheme
} from "@mui/material";
import type { SelectChangeEvent } from "@mui/material";
import { api } from "./api";
import type {
  AgentSummary,
  DebugEntry,
  DeliveryTarget,
  RunSummary,
  SessionDebug,
  SessionSummary,
  SessionTask,
  SessionTranscript,
  TelegramChat,
  ToolCallSummary,
  TraceLink,
  WebSnapshot
} from "./types";

type SectionId = "overview" | "sessions" | "agents" | "tasks" | "tools" | "routes" | "traces" | "settings";
type SessionPane = "timeline" | "transcript" | "debug" | "tasks" | "run";

type SessionEvent = {
  id: string;
  kind: string;
  label: string;
  detailTitle: string;
  detail: string;
  createdAt: number;
  runId?: string | null;
  artifactId?: string | null;
  source: "debug" | "transcript";
};

const drawerWidth = 276;

const theme = createTheme({
  palette: {
    mode: "dark",
    background: {
      default: "#0e1217",
      paper: "#151a21"
    },
    primary: {
      main: "#79c7b7",
      contrastText: "#071311"
    },
    secondary: {
      main: "#9eb6ff"
    },
    warning: {
      main: "#f2c36b"
    },
    error: {
      main: "#ff7b7b"
    },
    success: {
      main: "#84d28a"
    },
    divider: "rgba(255,255,255,0.08)"
  },
  typography: {
    fontFamily: '"IBM Plex Sans", "Segoe UI", "Noto Sans", sans-serif',
    fontSize: 13,
    h5: {
      fontWeight: 700,
      letterSpacing: "-0.02em"
    },
    h6: {
      fontWeight: 700
    },
    button: {
      textTransform: "none",
      fontWeight: 700
    }
  },
  shape: {
    borderRadius: 10
  },
  components: {
    MuiButton: {
      defaultProps: {
        size: "small"
      }
    },
    MuiChip: {
      defaultProps: {
        size: "small"
      }
    },
    MuiTextField: {
      defaultProps: {
        size: "small"
      }
    },
    MuiTableCell: {
      styleOverrides: {
        root: {
          borderColor: "rgba(255,255,255,0.07)",
          padding: "8px 10px",
          verticalAlign: "top"
        },
        head: {
          color: "rgba(255,255,255,0.72)",
          fontSize: 12,
          fontWeight: 700,
          letterSpacing: "0.02em",
          textTransform: "uppercase"
        }
      }
    },
    MuiPaper: {
      styleOverrides: {
        root: {
          backgroundImage: "none"
        }
      }
    }
  }
});

const sections: Array<{ id: SectionId; label: string; description: string }> = [
  { id: "overview", label: "Обзор", description: "runtime, NATS, Postgres" },
  { id: "sessions", label: "Сессии", description: "чат, transcript, debug" },
  { id: "agents", label: "Агенты", description: "профили и workspaces" },
  { id: "tasks", label: "Задачи", description: "registry и делегации" },
  { id: "tools", label: "Tools", description: "вызовы и ошибки" },
  { id: "routes", label: "Маршруты", description: "delivery targets" },
  { id: "traces", label: "Traces", description: "OTel ссылки" },
  { id: "settings", label: "Настройки", description: "read-only конфиг" }
];

function formatTime(value?: number | null): string {
  if (!value) {
    return "—";
  }
  return new Date(value * 1000).toLocaleString("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  });
}

function short(value?: string | null, size = 14): string {
  if (!value) {
    return "—";
  }
  return value.length > size ? `${value.slice(0, size)}…` : value;
}

function statusColor(status?: string | null): "success" | "warning" | "error" | "default" | "info" {
  const normalized = (status ?? "").toLowerCase();
  if (["completed", "done", "ok", "idle", "ready", "success"].includes(normalized)) {
    return "success";
  }
  if (["running", "queued", "in_progress", "active"].includes(normalized)) {
    return "warning";
  }
  if (["failed", "error", "cancelled", "killed"].includes(normalized)) {
    return "error";
  }
  return "default";
}

function parseJsonLabel(value?: string | null): string {
  if (!value) {
    return "—";
  }
  try {
    const parsed = JSON.parse(value);
    if (typeof parsed === "string") {
      return parsed;
    }
    if (parsed.goal) {
      return String(parsed.goal);
    }
    if (parsed.prompt) {
      return String(parsed.prompt);
    }
    return JSON.stringify(parsed);
  } catch {
    return value;
  }
}

function SectionHeader({
  title,
  subtitle,
  action
}: {
  title: string;
  subtitle?: string;
  action?: ReactNode;
}) {
  return (
    <Stack direction="row" alignItems="flex-start" justifyContent="space-between" spacing={2} sx={{ mb: 2 }}>
      <Box>
        <Typography variant="h5">{title}</Typography>
        {subtitle ? (
          <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
            {subtitle}
          </Typography>
        ) : null}
      </Box>
      {action}
    </Stack>
  );
}

function EmptyState({ title, detail }: { title: string; detail?: string }) {
  return (
    <Paper variant="outlined" sx={{ p: 3 }}>
      <Typography fontWeight={700}>{title}</Typography>
      {detail ? (
        <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
          {detail}
        </Typography>
      ) : null}
    </Paper>
  );
}

function Metric({ label, value, hint }: { label: string; value: ReactNode; hint?: string }) {
  return (
    <Paper variant="outlined" sx={{ p: 1.5, minWidth: 150 }}>
      <Typography variant="caption" color="text.secondary">
        {label}
      </Typography>
      <Typography variant="h6" sx={{ mt: 0.5 }}>
        {value}
      </Typography>
      {hint ? (
        <Typography variant="caption" color="text.secondary">
          {hint}
        </Typography>
      ) : null}
    </Paper>
  );
}

function StatusChip({ value }: { value?: string | null }) {
  return <Chip label={value || "unknown"} color={statusColor(value)} variant="outlined" />;
}

function JsonBlock({ value }: { value: unknown }) {
  return <pre className="json-block">{typeof value === "string" ? value : JSON.stringify(value, null, 2)}</pre>;
}

function eventTone(kind: string): "primary" | "secondary" | "warning" | "success" | "error" | "default" {
  const normalized = kind.toLowerCase();
  if (normalized.includes("tool")) {
    return "warning";
  }
  if (normalized.includes("artifact")) {
    return "secondary";
  }
  if (normalized.includes("assistant")) {
    return "primary";
  }
  if (normalized.includes("user")) {
    return "success";
  }
  if (normalized.includes("error") || normalized.includes("failed")) {
    return "error";
  }
  return "default";
}

function buildSessionEvents(debug: SessionDebug | null, transcript: SessionTranscript | null): SessionEvent[] {
  if (debug?.entries.length) {
    return debug.entries
      .map((entry) => ({
        id: entry.id,
        kind: entry.kind,
        label: entry.label,
        detailTitle: entry.detail_title,
        detail: entry.detail,
        createdAt: entry.created_at,
        runId: entry.run_id,
        artifactId: entry.artifact_id,
        source: "debug" as const
      }))
      .sort((left, right) => left.createdAt - right.createdAt || left.id.localeCompare(right.id));
  }

  return (transcript?.entries ?? [])
    .map((entry, index) => ({
      id: `transcript-${entry.created_at}-${index}`,
      kind: entry.tool_name ? "tool" : entry.role,
      label: entry.tool_name ?? entry.role,
      detailTitle: entry.tool_status ? `${entry.tool_name ?? "tool"} · ${entry.tool_status}` : entry.role,
      detail: entry.content,
      createdAt: entry.created_at,
      runId: entry.run_id,
      source: "transcript" as const
    }))
    .sort((left, right) => left.createdAt - right.createdAt || left.id.localeCompare(right.id));
}

function SessionsTable({
  sessions,
  selectedId,
  filter,
  onFilterChange,
  onSelect
}: {
  sessions: SessionSummary[];
  selectedId: string | null;
  filter: string;
  onFilterChange: (value: string) => void;
  onSelect: (id: string) => void;
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
      <TableContainer component={Paper} variant="outlined" sx={{ maxHeight: "calc(100vh - 260px)" }}>
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

function TranscriptPane({ transcript }: { transcript: SessionTranscript | null }) {
  if (!transcript || transcript.entries.length === 0) {
    return <EmptyState title="Transcript пуст" detail="В этой сессии пока нет сообщений." />;
  }
  return (
    <Stack spacing={1}>
      {transcript.entries.map((entry, index) => (
        <Paper key={`${entry.created_at}-${index}`} variant="outlined" className={`message-row role-${entry.role}`}>
          <Stack direction="row" spacing={1} alignItems="center" sx={{ mb: 0.75 }}>
            <Chip label={entry.role} color={entry.role === "assistant" ? "primary" : "default"} variant="outlined" />
            <Typography variant="caption" color="text.secondary">
              {formatTime(entry.created_at)}
            </Typography>
            {entry.run_id ? (
              <Typography variant="caption" color="text.secondary" className="mono">
                {short(entry.run_id, 18)}
              </Typography>
            ) : null}
            {entry.tool_name ? <Chip label={entry.tool_name} variant="outlined" /> : null}
            {entry.tool_status ? <StatusChip value={entry.tool_status} /> : null}
          </Stack>
          <Typography component="pre" className="transcript-text">
            {entry.content}
          </Typography>
        </Paper>
      ))}
    </Stack>
  );
}

function DebugPane({ debug }: { debug: SessionDebug | null }) {
  if (!debug || debug.entries.length === 0) {
    return <EmptyState title="Debug-лента пуста" detail="Нет сообщений, tool calls или артефактов для этой сессии." />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Тип</TableCell>
            <TableCell>Событие</TableCell>
            <TableCell>Детали</TableCell>
            <TableCell>Время</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {debug.entries.map((entry: DebugEntry) => (
            <TableRow key={entry.id} hover>
              <TableCell>
                <Chip label={entry.kind} variant="outlined" />
              </TableCell>
              <TableCell>
                <Typography fontWeight={700}>{entry.label}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {short(entry.id, 28)}
                </Typography>
              </TableCell>
              <TableCell sx={{ maxWidth: 520 }}>
                <Typography variant="body2" fontWeight={700}>
                  {entry.detail_title}
                </Typography>
                <Typography component="pre" className="debug-detail">
                  {entry.detail}
                </Typography>
              </TableCell>
              <TableCell>{formatTime(entry.created_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

function SessionTimeline({
  events,
  selectedEventId,
  onSelectEvent
}: {
  events: SessionEvent[];
  selectedEventId: string | null;
  onSelectEvent: (id: string) => void;
}) {
  if (events.length === 0) {
    return <EmptyState title="Timeline пуст" detail="Нет transcript/debug событий для выбранной сессии." />;
  }

  return (
    <Paper variant="outlined" className="timeline-panel">
      <Stack divider={<Divider flexItem />} spacing={0}>
        {events.map((event) => (
          <Box
            key={event.id}
            component="button"
            type="button"
            className={`timeline-event ${event.id === selectedEventId ? "is-selected" : ""}`}
            onClick={() => onSelectEvent(event.id)}
          >
            <Box className="timeline-marker" />
            <Box className="timeline-body">
              <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap>
                <Chip label={event.kind} color={eventTone(event.kind)} variant="outlined" />
                <Typography variant="caption" color="text.secondary">
                  {formatTime(event.createdAt)}
                </Typography>
                {event.runId ? (
                  <Typography variant="caption" color="text.secondary" className="mono">
                    run {short(event.runId, 18)}
                  </Typography>
                ) : null}
                {event.artifactId ? (
                  <Typography variant="caption" color="text.secondary" className="mono">
                    artifact {short(event.artifactId, 18)}
                  </Typography>
                ) : null}
              </Stack>
              <Typography fontWeight={700} sx={{ mt: 0.75 }}>
                {event.label || event.detailTitle}
              </Typography>
              <Typography variant="body2" color="text.secondary" noWrap>
                {event.detailTitle}
              </Typography>
              <Typography component="pre" className="timeline-preview">
                {event.detail}
              </Typography>
            </Box>
          </Box>
        ))}
      </Stack>
    </Paper>
  );
}

function KeyValueTable({ rows }: { rows: Array<[string, ReactNode]> }) {
  return (
    <Table size="small">
      <TableBody>
        {rows.map(([label, value]) => (
          <TableRow key={label}>
            <TableCell sx={{ width: 118, color: "text.secondary" }}>{label}</TableCell>
            <TableCell>{value}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}

function SessionInspector({
  session,
  selectedEvent,
  tasks,
  tools,
  run,
  onRefresh,
  onCancelRun,
  onCancelAll
}: {
  session: SessionSummary | null;
  selectedEvent: SessionEvent | null;
  tasks: SessionTask[];
  tools: ToolCallSummary[];
  run: unknown;
  onRefresh: () => void;
  onCancelRun: () => void;
  onCancelAll: () => void;
}) {
  if (!session) {
    return <EmptyState title="Inspector пуст" detail="Выбери сессию." />;
  }

  const sessionTools = tools.filter((tool) => tool.session_id === session.id);
  const failedTools = sessionTools.filter((tool) => tool.status !== "completed" || tool.error);
  const activeTasks = tasks.filter((task) => ["queued", "running", "in_progress"].includes(task.status));

  return (
    <Stack spacing={1.5} className="inspector-panel">
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction="row" justifyContent="space-between" spacing={1} alignItems="flex-start">
          <Box>
            <Typography variant="subtitle2" color="text.secondary">
              Выбранная сессия
            </Typography>
            <Typography fontWeight={800}>{session.title || "Без названия"}</Typography>
            <Typography variant="caption" color="text.secondary" className="mono">
              {session.id}
            </Typography>
          </Box>
          <Button variant="outlined" onClick={onRefresh}>
            Refresh
          </Button>
        </Stack>
        <Divider sx={{ my: 1.25 }} />
        <KeyValueTable
          rows={[
            ["Агент", `${session.agent_name} (${session.agent_profile_id})`],
            ["Модель", session.model || "—"],
            ["Сообщения", session.message_count],
            ["Контекст", session.context_tokens],
            ["Compact", session.compactifications],
            ["Auto approve", session.auto_approve ? "да" : "нет"],
            ["Обновлена", formatTime(session.updated_at)]
          ]}
        />
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Оперативное состояние
        </Typography>
        <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
          <Chip label={`tasks: ${tasks.length}`} variant="outlined" />
          <Chip label={`active: ${activeTasks.length}`} color={activeTasks.length > 0 ? "warning" : "default"} variant="outlined" />
          <Chip label={`tools: ${sessionTools.length}`} variant="outlined" />
          <Chip label={`tool errors: ${failedTools.length}`} color={failedTools.length > 0 ? "error" : "default"} variant="outlined" />
          {session.has_pending_approval ? <Chip label="approval pending" color="warning" /> : null}
        </Stack>
        <Stack direction="row" spacing={1} sx={{ mt: 1.25 }}>
          <Button color="warning" variant="outlined" onClick={onCancelRun}>
            Stop run
          </Button>
          <Button color="error" variant="outlined" onClick={onCancelAll}>
            Cancel all
          </Button>
        </Stack>
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Выбранное событие
        </Typography>
        {selectedEvent ? (
          <Stack spacing={1}>
            <Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap>
              <Chip label={selectedEvent.kind} color={eventTone(selectedEvent.kind)} variant="outlined" />
              <Typography variant="caption" color="text.secondary">
                {formatTime(selectedEvent.createdAt)}
              </Typography>
            </Stack>
            <Typography fontWeight={700}>{selectedEvent.label}</Typography>
            <Typography variant="body2" color="text.secondary">
              {selectedEvent.detailTitle}
            </Typography>
            <Typography variant="caption" color="text.secondary" className="mono">
              {selectedEvent.id}
            </Typography>
            <Typography component="pre" className="inspector-detail">
              {selectedEvent.detail}
            </Typography>
          </Stack>
        ) : (
          <Typography variant="body2" color="text.secondary">
            Выбери событие в timeline.
          </Typography>
        )}
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Typography fontWeight={700} sx={{ mb: 1 }}>
          Active run raw
        </Typography>
        <JsonBlock value={run ?? "Нет активного run."} />
      </Paper>
    </Stack>
  );
}

function TasksPane({ tasks }: { tasks: SessionTask[] }) {
  if (tasks.length === 0) {
    return <EmptyState title="Задач нет" detail="Task registry для выбранной сессии пуст." />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Task</TableCell>
            <TableCell>Статус</TableCell>
            <TableCell>Исполнитель</TableCell>
            <TableCell>Контекст</TableCell>
            <TableCell>Обновлена</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {tasks.map((task) => (
            <TableRow key={task.id} hover>
              <TableCell>
                <Typography fontWeight={700}>{task.kind}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {short(task.id, 32)}
                </Typography>
              </TableCell>
              <TableCell>
                <StatusChip value={task.status} />
                <Typography variant="caption" color="text.secondary" display="block" sx={{ mt: 0.5 }}>
                  {task.attempt_count}/{task.max_attempts} попыток
                </Typography>
              </TableCell>
              <TableCell>{task.executor_agent_id || task.owner_agent_id || "—"}</TableCell>
              <TableCell sx={{ maxWidth: 420 }}>
                <Typography variant="body2" noWrap>
                  {parseJsonLabel(task.context_ref_json)}
                </Typography>
                {task.error ? (
                  <Typography variant="caption" color="error">
                    {task.error}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell>{formatTime(task.updated_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

function ToolsTable({ tools, filter, onFilterChange }: { tools: ToolCallSummary[]; filter: string; onFilterChange: (value: string) => void }) {
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

function AgentsTable({
  agents,
  onCreate
}: {
  agents: AgentSummary[];
  onCreate: () => void;
}) {
  return (
    <Stack spacing={1.5}>
      <Stack direction="row" justifyContent="space-between" alignItems="center">
        <Typography variant="body2" color="text.secondary">
          Сейчас доступны создание профиля и просмотр workspace. Редактирование SYSTEM/AGENTS/skills будет отдельным API поверх agentd.
        </Typography>
        <Button variant="contained" onClick={onCreate}>
          Создать агента
        </Button>
      </Stack>
      <TableContainer component={Paper} variant="outlined">
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>ID</TableCell>
              <TableCell>Имя</TableCell>
              <TableCell>Шаблон</TableCell>
              <TableCell>Workspace</TableCell>
              <TableCell>Обновлён</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {agents.map((agent) => (
              <TableRow key={agent.id} hover>
                <TableCell className="mono">{agent.id}</TableCell>
                <TableCell>{agent.name}</TableCell>
                <TableCell>{agent.template_kind}</TableCell>
                <TableCell className="mono">{agent.default_workspace_root || "—"}</TableCell>
                <TableCell>{formatTime(agent.updated_at)}</TableCell>
              </TableRow>
            ))}
            {agents.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5}>
                  <EmptyState title="Агентов нет" detail="Snapshot не вернул agent profiles." />
                </TableCell>
              </TableRow>
            ) : null}
          </TableBody>
        </Table>
      </TableContainer>
    </Stack>
  );
}

function RoutesView({ targets, chats }: { targets: DeliveryTarget[]; chats: TelegramChat[] }) {
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

function TracesTable({ traces }: { traces: TraceLink[] }) {
  if (traces.length === 0) {
    return <EmptyState title="Trace links нет" detail="Когда runtime создаст trace links, они появятся здесь." />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Trace</TableCell>
            <TableCell>Span</TableCell>
            <TableCell>Entity</TableCell>
            <TableCell>Surface</TableCell>
            <TableCell>Время</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {traces.map((trace) => (
            <TableRow key={`${trace.trace_id}-${trace.span_id}-${trace.entity_id}`} hover>
              <TableCell className="mono">{short(trace.trace_id, 28)}</TableCell>
              <TableCell className="mono">{short(trace.span_id, 18)}</TableCell>
              <TableCell>
                <Typography>{trace.entity_kind}</Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  {short(trace.entity_id, 32)}
                </Typography>
              </TableCell>
              <TableCell>
                {trace.surface || "—"}
                {trace.entrypoint ? (
                  <Typography variant="caption" display="block" color="text.secondary">
                    {trace.entrypoint}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell>{formatTime(trace.created_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

function RunsTable({ runs }: { runs: RunSummary[] }) {
  if (runs.length === 0) {
    return <EmptyState title="Run history пуст" />;
  }
  return (
    <TableContainer component={Paper} variant="outlined">
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Run</TableCell>
            <TableCell>Сессия</TableCell>
            <TableCell>Статус</TableCell>
            <TableCell>Начат</TableCell>
            <TableCell>Обновлён</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {runs.map((run) => (
            <TableRow key={run.id} hover>
              <TableCell className="mono">{short(run.id, 32)}</TableCell>
              <TableCell className="mono">{short(run.session_id, 28)}</TableCell>
              <TableCell>
                <StatusChip value={run.status} />
                {run.error ? (
                  <Typography variant="caption" color="error" display="block" sx={{ mt: 0.5 }}>
                    {run.error}
                  </Typography>
                ) : null}
              </TableCell>
              <TableCell>{formatTime(run.started_at)}</TableCell>
              <TableCell>{formatTime(run.updated_at)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </TableContainer>
  );
}

export function App() {
  const [section, setSection] = useState<SectionId>("overview");
  const [snapshot, setSnapshot] = useState<WebSnapshot | null>(null);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [transcript, setTranscript] = useState<SessionTranscript | null>(null);
  const [debug, setDebug] = useState<SessionDebug | null>(null);
  const [tasks, setTasks] = useState<SessionTask[]>([]);
  const [run, setRun] = useState<unknown>(null);
  const [sessionPane, setSessionPane] = useState<SessionPane>("timeline");
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [sessionFilter, setSessionFilter] = useState("");
  const [toolFilter, setToolFilter] = useState("");
  const [message, setMessage] = useState("");
  const [sending, setSending] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [createSessionOpen, setCreateSessionOpen] = useState(false);
  const [newSessionTitle, setNewSessionTitle] = useState("Новая web-сессия");
  const [newSessionAgent, setNewSessionAgent] = useState("");
  const [createAgentOpen, setCreateAgentOpen] = useState(false);
  const [newAgentName, setNewAgentName] = useState("");
  const [newAgentTemplate, setNewAgentTemplate] = useState("default");

  async function loadData(signal?: AbortSignal) {
    setLoading(true);
    setError(null);
    try {
      const [nextSnapshot, nextSessions] = await Promise.all([api.snapshot(signal), api.sessions(signal)]);
      setSnapshot(nextSnapshot);
      setSessions(nextSessions);
      setSelectedSessionId((current) => {
        if (current && nextSessions.some((session) => session.id === current)) {
          return current;
        }
        return nextSessions[0]?.id ?? null;
      });
    } catch (loadError) {
      if (!signal?.aborted) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setLoading(false);
      }
    }
  }

  async function loadSessionDetails(sessionId: string, signal?: AbortSignal) {
    setDetailLoading(true);
    setDetailError(null);
    try {
      const [nextTranscript, nextDebug, nextTasks, nextRun] = await Promise.all([
        api.transcript(sessionId, 180, signal),
        api.debug(sessionId, signal),
        api.tasks(sessionId, signal),
        api.run(sessionId, signal).catch((runError) => ({ error: runError instanceof Error ? runError.message : String(runError) }))
      ]);
      setTranscript(nextTranscript);
      setDebug(nextDebug);
      setTasks(nextTasks);
      setRun(nextRun);
    } catch (loadError) {
      if (!signal?.aborted) {
        setDetailError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setDetailLoading(false);
      }
    }
  }

  useEffect(() => {
    const controller = new AbortController();
    void loadData(controller.signal);
    const timer = window.setInterval(() => void loadData(controller.signal), 10_000);
    return () => {
      controller.abort();
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!selectedSessionId) {
      setTranscript(null);
      setDebug(null);
      setTasks([]);
      setRun(null);
      setSelectedEventId(null);
      return;
    }
    const controller = new AbortController();
    void loadSessionDetails(selectedSessionId, controller.signal);
    return () => controller.abort();
  }, [selectedSessionId]);

  const selectedSession = sessions.find((session) => session.id === selectedSessionId) ?? null;
  const toolErrors = snapshot?.recent_tool_calls.filter((tool) => tool.status !== "completed" || tool.error).length ?? 0;
  const activeRuns = snapshot?.recent_runs.filter((runItem) => ["running", "queued"].includes(runItem.status)).length ?? 0;
  const sessionEvents = buildSessionEvents(debug, transcript);
  const selectedEvent =
    sessionEvents.find((event) => event.id === selectedEventId) ?? sessionEvents[sessionEvents.length - 1] ?? null;

  useEffect(() => {
    if (sessionEvents.length === 0) {
      setSelectedEventId(null);
      return;
    }
    setSelectedEventId((current) => {
      if (current && sessionEvents.some((event) => event.id === current)) {
        return current;
      }
      return sessionEvents[sessionEvents.length - 1].id;
    });
  }, [debug, transcript]);

  async function submitMessage() {
    const trimmed = message.trim();
    if (!selectedSessionId || !trimmed) {
      return;
    }
    setSending(true);
    setNotice(null);
    try {
      const result = await api.sendMessage(selectedSessionId, trimmed);
      setMessage("");
      setNotice(result.kind === "chat_completed" ? "Ответ получен, transcript обновлён." : `Runtime вернул: ${result.kind}`);
      await loadData();
      await loadSessionDetails(selectedSessionId);
    } catch (sendError) {
      setNotice(sendError instanceof Error ? sendError.message : String(sendError));
    } finally {
      setSending(false);
    }
  }

  async function submitCreateSession() {
    const title = newSessionTitle.trim() || "Новая web-сессия";
    try {
      const created = await api.createSession(title, newSessionAgent || undefined);
      setCreateSessionOpen(false);
      setSelectedSessionId(created.id);
      setSection("sessions");
      setNotice(`Сессия создана: ${created.title}`);
      await loadData();
    } catch (createError) {
      setNotice(createError instanceof Error ? createError.message : String(createError));
    }
  }

  async function submitCreateAgent() {
    const name = newAgentName.trim();
    if (!name) {
      setNotice("Укажи имя агента.");
      return;
    }
    try {
      const result = await api.createAgent(name, newAgentTemplate || undefined);
      setCreateAgentOpen(false);
      setNewAgentName("");
      setNotice(result.message);
      await loadData();
    } catch (createError) {
      setNotice(createError instanceof Error ? createError.message : String(createError));
    }
  }

  async function cancelRun(all: boolean) {
    if (!selectedSessionId) {
      return;
    }
    try {
      await (all ? api.cancelAllWork(selectedSessionId) : api.cancelRun(selectedSessionId));
      setNotice(all ? "Запрошена отмена всей работы сессии." : "Запрошена отмена активного run.");
      await loadSessionDetails(selectedSessionId);
      await loadData();
    } catch (cancelError) {
      setNotice(cancelError instanceof Error ? cancelError.message : String(cancelError));
    }
  }

  function renderOverview() {
    if (!snapshot) {
      return <EmptyState title="Snapshot недоступен" detail="Нет данных от /v1/web/snapshot." />;
    }
    return (
      <Stack spacing={2}>
        <SectionHeader
          title="Обзор runtime"
          subtitle="Read-only состояние agentd, NATS, Postgres и последних операций."
          action={
            <Button variant="outlined" onClick={() => void loadData()} disabled={loading}>
              Обновить
            </Button>
          }
        />
        <Stack direction="row" spacing={1.5} flexWrap="wrap" useFlexGap>
          <Metric label="Сессии" value={snapshot.status.session_count} hint={`${sessions.length} в списке`} />
          <Metric label="Runs" value={snapshot.status.run_count} hint={`${activeRuns} активных`} />
          <Metric label="Jobs" value={snapshot.status.job_count} />
          <Metric label="Tool errors" value={toolErrors} hint={`${snapshot.recent_tool_calls.length} последних вызовов`} />
          <Metric label="Agents" value={snapshot.agents.length} />
          <Metric label="DB" value={snapshot.status.database || "—"} />
        </Stack>

        <Paper variant="outlined" sx={{ p: 1.5 }}>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={`version: ${snapshot.status.version ?? "unknown"}`} variant="outlined" />
            <Chip label={`commit: ${short(snapshot.status.commit, 12)}`} variant="outlined" />
            <Chip label={`tree: ${snapshot.status.tree_state ?? "unknown"}`} color={snapshot.status.tree_state === "clean" ? "success" : "warning"} variant="outlined" />
            <Chip label={`permission: ${snapshot.status.permission_mode}`} variant="outlined" />
            <Chip label={`event bus: ${snapshot.event_bus.backend}`} color={snapshot.event_bus.nats_configured ? "success" : "warning"} variant="outlined" />
            <Chip label={`generated: ${formatTime(snapshot.generated_at)}`} variant="outlined" />
          </Stack>
        </Paper>

        <Box className="grid-two">
          <Paper variant="outlined" sx={{ p: 1.5 }}>
            <Typography fontWeight={700} sx={{ mb: 1 }}>
              Event bus
            </Typography>
            <Table size="small">
              <TableBody>
                <TableRow>
                  <TableCell>Backend</TableCell>
                  <TableCell>{snapshot.event_bus.backend}</TableCell>
                </TableRow>
                <TableRow>
                  <TableCell>Required</TableCell>
                  <TableCell>{snapshot.event_bus.required ? "да" : "нет"}</TableCell>
                </TableRow>
                <TableRow>
                  <TableCell>NATS</TableCell>
                  <TableCell>{snapshot.event_bus.nats_configured ? "настроен" : "не настроен"}</TableCell>
                </TableRow>
                <TableRow>
                  <TableCell>Streams</TableCell>
                  <TableCell className="mono">
                    {[
                      snapshot.event_bus.input_stream,
                      snapshot.event_bus.session_stream,
                      snapshot.event_bus.delivery_stream,
                      snapshot.event_bus.task_stream,
                      snapshot.event_bus.dlq_stream
                    ].join(" · ")}
                  </TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </Paper>
          <Paper variant="outlined" sx={{ p: 1.5 }}>
            <Typography fontWeight={700} sx={{ mb: 1 }}>
              Runtime paths
            </Typography>
            <Table size="small">
              <TableBody>
                <TableRow>
                  <TableCell>Data dir</TableCell>
                  <TableCell className="mono">{snapshot.status.data_dir}</TableCell>
                </TableRow>
                <TableRow>
                  <TableCell>Build</TableCell>
                  <TableCell className="mono">{snapshot.status.build_id || "—"}</TableCell>
                </TableRow>
                <TableRow>
                  <TableCell>Traces</TableCell>
                  <TableCell>{snapshot.recent_traces.length}</TableCell>
                </TableRow>
                <TableRow>
                  <TableCell>Telegram chats</TableCell>
                  <TableCell>{snapshot.telegram_chats.length}</TableCell>
                </TableRow>
              </TableBody>
            </Table>
          </Paper>
        </Box>

        <RunsTable runs={snapshot.recent_runs} />
      </Stack>
    );
  }

  function renderSessions() {
    return (
      <Stack spacing={2}>
        <SectionHeader
          title="Сессии"
          subtitle="Операторский экран: список → timeline → inspector. Сообщения идут только через canonical /v1/chat/turn."
          action={
            <Stack direction="row" spacing={1}>
              <Button variant="outlined" onClick={() => void loadData()} disabled={loading}>
                Обновить
              </Button>
              <Button variant="contained" onClick={() => setCreateSessionOpen(true)}>
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
              onFilterChange={setSessionFilter}
              onSelect={setSelectedSessionId}
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
                      onChange={(event) => setMessage(event.target.value)}
                      placeholder="Введите команду или вопрос..."
                      onKeyDown={(event) => {
                        if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
                          void submitMessage();
                        }
                      }}
                    />
                    <Button variant="contained" onClick={() => void submitMessage()} disabled={sending || !message.trim()}>
                      {sending ? "Отправка..." : "Отправить"}
                    </Button>
                  </Stack>
                </Paper>

                <Paper variant="outlined">
                  <Tabs
                    value={sessionPane}
                    onChange={(_, value: SessionPane) => setSessionPane(value)}
                    variant="scrollable"
                    scrollButtons="auto"
                  >
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
                  <SessionTimeline events={sessionEvents} selectedEventId={selectedEvent?.id ?? null} onSelectEvent={setSelectedEventId} />
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
              tools={snapshot?.recent_tool_calls ?? []}
              run={run}
              onRefresh={() => {
                if (selectedSessionId) {
                  void loadSessionDetails(selectedSessionId);
                }
                void loadData();
              }}
              onCancelRun={() => void cancelRun(false)}
              onCancelAll={() => void cancelRun(true)}
            />
          </Box>
        </Box>
      </Stack>
    );
  }

  function renderContent() {
    if (error) {
      return (
        <Stack spacing={2}>
          <Alert severity="error">{error}</Alert>
          <Button variant="contained" onClick={() => void loadData()}>
            Повторить
          </Button>
        </Stack>
      );
    }

    switch (section) {
      case "overview":
        return renderOverview();
      case "sessions":
        return renderSessions();
      case "agents":
        return (
          <>
            <SectionHeader title="Агенты" subtitle="Agent profiles из canonical runtime." />
            <AgentsTable agents={snapshot?.agents ?? []} onCreate={() => setCreateAgentOpen(true)} />
          </>
        );
      case "tasks":
        return (
          <>
            <SectionHeader
              title="Task registry"
              subtitle={selectedSession ? `Задачи выбранной сессии: ${selectedSession.title}` : "Выбери сессию на вкладке Сессии."}
            />
            <TasksPane tasks={tasks} />
          </>
        );
      case "tools":
        return (
          <>
            <SectionHeader title="Tool calls" subtitle="Последние вызовы инструментов из /v1/web/snapshot." />
            <ToolsTable tools={snapshot?.recent_tool_calls ?? []} filter={toolFilter} onFilterChange={setToolFilter} />
          </>
        );
      case "routes":
        return (
          <>
            <SectionHeader title="Маршруты доставки" subtitle="Delivery targets и Telegram bindings." />
            <RoutesView targets={snapshot?.delivery_targets ?? []} chats={snapshot?.telegram_chats ?? []} />
          </>
        );
      case "traces":
        return (
          <>
            <SectionHeader title="Traces" subtitle="Ссылки на trace/span, которые можно сопоставлять с Jaeger/OTel." />
            <TracesTable traces={snapshot?.recent_traces ?? []} />
          </>
        );
      case "settings":
        return (
          <>
            <SectionHeader title="Настройки" subtitle="Пока read-only. Запись настроек должна идти через отдельные agentd endpoints." />
            <JsonBlock
              value={{
                agentd_proxy: "/api/agentd/v1/*",
                canonical_chat_path: "/v1/chat/turn",
                snapshot: snapshot ?? null
              }}
            />
          </>
        );
      default:
        return null;
    }
  }

  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <Box className="app-shell">
        <AppBar position="fixed" color="default" elevation={0} sx={{ zIndex: (muiTheme) => muiTheme.zIndex.drawer + 1 }}>
          <Toolbar variant="dense" sx={{ gap: 1.5 }}>
            <Typography variant="h6" sx={{ flexGrow: 1 }}>
              teamD Web Console
            </Typography>
            {loading ? <CircularProgress size={18} /> : null}
            <Chip
              label={snapshot?.status.ok ? "agentd online" : "agentd unknown"}
              color={snapshot?.status.ok ? "success" : "warning"}
              variant="outlined"
            />
            <Chip label={`sessions: ${sessions.length}`} variant="outlined" />
            <Chip label={`tools err: ${toolErrors}`} color={toolErrors > 0 ? "warning" : "default"} variant="outlined" />
          </Toolbar>
        </AppBar>

        <Box component="nav" className="sidebar" sx={{ width: drawerWidth }}>
          <Toolbar variant="dense" />
          <Box sx={{ p: 1.25 }}>
            <List dense disablePadding>
              {sections.map((item) => (
                <ListItemButton key={item.id} selected={section === item.id} onClick={() => setSection(item.id)} sx={{ borderRadius: 1.5, mb: 0.5 }}>
                  <ListItemText
                    primary={item.label}
                    secondary={item.description}
                    primaryTypographyProps={{ fontWeight: 700 }}
                    secondaryTypographyProps={{ fontSize: 11 }}
                  />
                </ListItemButton>
              ))}
            </List>
          </Box>
          <Divider />
          <Box sx={{ p: 1.5 }}>
            <Typography variant="caption" color="text.secondary">
              Runtime
            </Typography>
            <Typography variant="body2" className="mono" sx={{ mt: 0.5 }}>
              {snapshot?.status.version ?? "—"} · {short(snapshot?.status.commit, 10)}
            </Typography>
            <Typography variant="caption" color="text.secondary">
              {snapshot?.status.data_dir ?? "snapshot не загружен"}
            </Typography>
          </Box>
        </Box>

        <Box component="main" className="main-panel" sx={{ ml: `${drawerWidth}px` }}>
          <Toolbar variant="dense" />
          {loading && !snapshot ? <LinearProgress sx={{ mb: 2 }} /> : null}
          {renderContent()}
        </Box>

        <Dialog open={createSessionOpen} onClose={() => setCreateSessionOpen(false)} maxWidth="sm" fullWidth>
          <DialogTitle>Новая сессия</DialogTitle>
          <DialogContent>
            <Stack spacing={2} sx={{ mt: 1 }}>
              <TextField label="Название" value={newSessionTitle} onChange={(event) => setNewSessionTitle(event.target.value)} />
              <FormControl size="small">
                <InputLabel id="session-agent-label">Агент</InputLabel>
                <Select
                  labelId="session-agent-label"
                  label="Агент"
                  value={newSessionAgent}
                  onChange={(event: SelectChangeEvent) => setNewSessionAgent(event.target.value)}
                >
                  <MenuItem value="">default runtime</MenuItem>
                  {(snapshot?.agents ?? []).map((agent) => (
                    <MenuItem key={agent.id} value={agent.id}>
                      {agent.name} ({agent.id})
                    </MenuItem>
                  ))}
                </Select>
              </FormControl>
            </Stack>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => setCreateSessionOpen(false)}>Отмена</Button>
            <Button variant="contained" onClick={() => void submitCreateSession()}>
              Создать
            </Button>
          </DialogActions>
        </Dialog>

        <Dialog open={createAgentOpen} onClose={() => setCreateAgentOpen(false)} maxWidth="sm" fullWidth>
          <DialogTitle>Создать агента</DialogTitle>
          <DialogContent>
            <Stack spacing={2} sx={{ mt: 1 }}>
              <TextField label="Имя агента" value={newAgentName} onChange={(event) => setNewAgentName(event.target.value)} />
              <FormControl size="small">
                <InputLabel id="agent-template-label">Шаблон</InputLabel>
                <Select
                  labelId="agent-template-label"
                  label="Шаблон"
                  value={newAgentTemplate}
                  onChange={(event: SelectChangeEvent) => setNewAgentTemplate(event.target.value)}
                >
                  <MenuItem value="default">default</MenuItem>
                  <MenuItem value="judge">judge</MenuItem>
                </Select>
              </FormControl>
              <Alert severity="info">
                Создание идёт через `/v1/agents`. Редактирование файлов профиля и skills будет добавлено отдельными endpoints, чтобы не делать второй runtime.
              </Alert>
            </Stack>
          </DialogContent>
          <DialogActions>
            <Button onClick={() => setCreateAgentOpen(false)}>Отмена</Button>
            <Button variant="contained" onClick={() => void submitCreateAgent()}>
              Создать
            </Button>
          </DialogActions>
        </Dialog>

        {notice ? (
          <Alert
            severity={notice.toLowerCase().includes("error") || notice.includes("Ошибка") ? "error" : "info"}
            onClose={() => setNotice(null)}
            className="notice"
          >
            {notice}
          </Alert>
        ) : null}
      </Box>
    </ThemeProvider>
  );
}
