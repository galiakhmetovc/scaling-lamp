import { Button, Chip, Paper, Stack, Typography } from "@mui/material";
import { KeyValueTable, StatusChip } from "../../components/common";
import type { DebugEntry, PendingApproval, SessionSummary, SessionTask, ToolCallSummary } from "../../types";
import { formatTime } from "../../utils/format";
import { ToolDetailPanel } from "./ToolDetailPanel";
import { buildToolStats } from "./toolStats";

export function ChatStatusPanel({
  selectedSession,
  tasks,
  tools,
  pendingApprovals,
  selectedToolId,
  toolDetails,
  onSelectTool,
  onClearTool,
  onCancelRun,
  onCancelAll
}: {
  selectedSession: SessionSummary | null;
  tasks: SessionTask[];
  tools: ToolCallSummary[];
  pendingApprovals: PendingApproval[];
  selectedToolId: string | null;
  toolDetails: DebugEntry | null;
  onSelectTool: (toolId: string) => void;
  onClearTool: () => void;
  onCancelRun: () => void;
  onCancelAll: () => void;
}) {
  const activeTasks = tasks.filter((task) => ["queued", "running", "in_progress"].includes(task.status));
  const selectedSessionTools = selectedSession ? tools.filter((tool) => tool.session_id === selectedSession.id) : [];
  const selectedSessionToolErrors = selectedSessionTools.filter((tool) => tool.status !== "completed" || tool.error);
  const stats = buildToolStats(selectedSessionTools);
  const selectedTool = selectedToolId ? selectedSessionTools.find((tool) => tool.id === selectedToolId) ?? null : null;

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
            {selectedSessionTools.slice(0, 10).map((tool) => (
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
              </button>
            ))}
          </Stack>
        )}
      </Paper>

      {selectedTool ? (
        <ToolDetailPanel tool={selectedTool} toolDetails={toolDetails} onClearTool={onClearTool} />
      ) : null}
    </Stack>
  );
}
