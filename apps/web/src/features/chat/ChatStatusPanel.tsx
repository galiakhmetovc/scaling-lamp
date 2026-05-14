import { useEffect, useState } from "react";
import {
  Button,
  Chip,
  Dialog,
  DialogContent,
  DialogTitle,
  FormControlLabel,
  IconButton,
  MenuItem,
  Pagination,
  Paper,
  Stack,
  Switch,
  TextField,
  Typography
} from "@mui/material";
import { KeyValueTable, StatusChip } from "../../components/common";
import type { DebugEntry, PendingApproval, SessionPreferencesPatch, SessionSummary, SessionTask, ToolCallSummary } from "../../types";
import { formatTime } from "../../utils/format";
import { ToolDetailPanel } from "./ToolDetailPanel";
import { buildToolStats } from "./toolStats";

const TOOL_PAGE_SIZE = 8;
const THINK_LEVELS = ["default", "off", "low", "medium", "high"];

export function ChatStatusPanel({
  selectedSession,
  tasks,
  tools,
  pendingApprovals,
  selectedToolId,
  toolDetails,
  debugEntries = [],
  onSelectTool,
  onClearTool,
  onUpdateSessionPreferences,
  onCancelRun,
  onCancelAll
}: {
  selectedSession: SessionSummary | null;
  tasks: SessionTask[];
  tools: ToolCallSummary[];
  pendingApprovals: PendingApproval[];
  selectedToolId: string | null;
  toolDetails: DebugEntry | null;
  debugEntries?: DebugEntry[];
  onSelectTool: (toolId: string) => void;
  onClearTool: () => void;
  onUpdateSessionPreferences: (patch: SessionPreferencesPatch) => void;
  onCancelRun: () => void;
  onCancelAll: () => void;
}) {
  const [toolsPage, setToolsPage] = useState(1);
  const [titleDraft, setTitleDraft] = useState("");
  const [modelDraft, setModelDraft] = useState("");
  const activeTasks = tasks.filter((task) => ["queued", "running", "in_progress"].includes(task.status));
  const selectedSessionTools = selectedSession
    ? tools
        .filter((tool) => tool.session_id === selectedSession.id)
        .sort((left, right) => (right.updated_at || right.requested_at || 0) - (left.updated_at || left.requested_at || 0))
    : [];
  const selectedSessionToolErrors = selectedSessionTools.filter((tool) => tool.status !== "completed" || tool.error);
  const stats = buildToolStats(selectedSessionTools);
  const selectedTool = selectedToolId ? selectedSessionTools.find((tool) => tool.id === selectedToolId) ?? null : null;
  const pageCount = Math.max(1, Math.ceil(selectedSessionTools.length / TOOL_PAGE_SIZE));
  const currentPage = Math.min(toolsPage, pageCount);
  const visibleTools = selectedSessionTools.slice((currentPage - 1) * TOOL_PAGE_SIZE, currentPage * TOOL_PAGE_SIZE);

  useEffect(() => {
    setToolsPage(1);
    setTitleDraft(selectedSession?.title ?? "");
    setModelDraft(selectedSession?.model ?? "");
  }, [selectedSession?.id, selectedSession?.model, selectedSession?.title]);

  return (
    <Stack spacing={1.25}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        {selectedSession ? (
          <Stack spacing={1.25}>
            <Typography fontWeight={700}>Общий статус</Typography>
            <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
              <Chip label={`сообщений: ${selectedSession.message_count}`} variant="outlined" />
              <Chip label={`context: ${selectedSession.context_tokens}`} variant="outlined" />
              <Chip label={`compact: ${selectedSession.compactifications}`} variant="outlined" />
              <Chip label={`tasks: ${tasks.length}`} variant="outlined" />
              <Chip label={`active: ${activeTasks.length}`} color={activeTasks.length ? "warning" : "default"} variant="outlined" />
              <Chip label={`tools ok: ${stats.succeeded}/${stats.total}`} color={stats.failed ? "warning" : "default"} variant="outlined" />
              <Chip label={`MCP ok: ${stats.mcpSucceeded}/${stats.mcpTotal}`} color={stats.mcpFailed ? "warning" : "default"} variant="outlined" />
              <Chip
                label={`tool errors: ${selectedSessionToolErrors.length}`}
                color={selectedSessionToolErrors.length ? "error" : "default"}
                variant="outlined"
              />
              {selectedSession.has_pending_approval || pendingApprovals.length > 0 ? <Chip label="approval pending" color="warning" /> : null}
            </Stack>
            <KeyValueTable
              rows={[
                ["Агент", `${selectedSession.agent_name} (${selectedSession.agent_profile_id})`],
                ["Модель", selectedSession.model || "—"],
                ["Think level", selectedSession.think_level || "default"],
                ["Reasoning visible", selectedSession.reasoning_visible ? "да" : "нет"],
                ["Auto approve", selectedSession.auto_approve ? "да" : "нет"],
                ["Context tokens", String(selectedSession.context_tokens)],
                [
                  "Usage",
                  selectedSession.usage_total_tokens
                    ? `${selectedSession.usage_input_tokens ?? 0}/${selectedSession.usage_output_tokens ?? 0}/${selectedSession.usage_total_tokens}`
                    : "—"
                ],
                ["Compactifications", String(selectedSession.compactifications)],
                ["Обновлена", formatTime(selectedSession.updated_at)]
              ]}
            />
            <Stack spacing={1}>
              <TextField
                size="small"
                label="Название"
                value={titleDraft}
                onChange={(event) => setTitleDraft(event.target.value)}
              />
              <Button
                size="small"
                variant="outlined"
                disabled={!titleDraft.trim() || titleDraft.trim() === selectedSession.title}
                onClick={() => onUpdateSessionPreferences({ title: titleDraft.trim() })}
              >
                Переименовать
              </Button>
              <TextField
                size="small"
                label="Модель"
                value={modelDraft}
                onChange={(event) => setModelDraft(event.target.value)}
                placeholder="default или имя модели"
              />
              <Button
                size="small"
                variant="outlined"
                disabled={(modelDraft.trim() || "") === (selectedSession.model ?? "")}
                onClick={() => onUpdateSessionPreferences({ model: modelDraft.trim() || null })}
              >
                Сменить модель
              </Button>
              <TextField
                select
                size="small"
                label="Think level"
                value={selectedSession.think_level ?? "default"}
                onChange={(event) =>
                  onUpdateSessionPreferences({
                    think_level: event.target.value === "default" ? null : event.target.value
                  })
                }
              >
                {THINK_LEVELS.map((level) => (
                  <MenuItem key={level} value={level}>
                    {level}
                  </MenuItem>
                ))}
              </TextField>
              <FormControlLabel
                control={
                  <Switch
                    checked={selectedSession.auto_approve}
                    onChange={(event) => onUpdateSessionPreferences({ auto_approve: event.target.checked })}
                  />
                }
                label="Auto-approve"
              />
            </Stack>
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
        <Stack direction="row" spacing={1} alignItems="center" justifyContent="space-between" sx={{ mb: 1 }}>
          <Typography fontWeight={700}>Последние tools</Typography>
        </Stack>
        {selectedSessionTools.length === 0 ? (
          <Typography variant="body2" color="text.secondary">
            Нет tool calls в snapshot.
          </Typography>
        ) : (
          <Stack spacing={1}>
            {visibleTools.map((tool) => (
              <button
                key={tool.id}
                className={`chat-tool-row ${tool.id === selectedToolId ? "is-selected" : ""}`}
                type="button"
                onClick={() => onSelectTool(tool.id)}
              >
                <Stack direction="row" justifyContent="space-between" spacing={1}>
                  <Typography className="mono" fontWeight={700}>
                    {tool.tool_name}
                  </Typography>
                  <StatusChip value={tool.status} />
                </Stack>
                <Typography variant="caption" color={tool.error ? "error" : "text.secondary"}>
                  {tool.error || tool.summary}
                </Typography>
                <Typography variant="caption" color="text.secondary" className="mono">
                  старт: {formatTime(tool.requested_at)} · обновлено: {formatTime(tool.updated_at)}
                </Typography>
              </button>
            ))}
            {selectedSessionTools.length > TOOL_PAGE_SIZE ? (
              <Pagination
                size="small"
                count={pageCount}
                page={currentPage}
                onChange={(_, page) => setToolsPage(page)}
              />
            ) : null}
          </Stack>
        )}
      </Paper>

      <Dialog open={Boolean(selectedTool)} onClose={onClearTool} fullWidth maxWidth="lg">
        <DialogTitle>
          <Stack direction="row" alignItems="center" justifyContent="space-between" spacing={2}>
            <Typography fontWeight={700}>Детали tool call</Typography>
            <IconButton aria-label="Закрыть" onClick={onClearTool}>
              ×
            </IconButton>
          </Stack>
        </DialogTitle>
        <DialogContent dividers>
          {selectedTool ? (
            <ToolDetailPanel
              tool={selectedTool}
              toolDetails={toolDetails}
              allTools={selectedSessionTools}
              debugEntries={debugEntries}
              onClearTool={onClearTool}
            />
          ) : null}
        </DialogContent>
      </Dialog>
    </Stack>
  );
}
