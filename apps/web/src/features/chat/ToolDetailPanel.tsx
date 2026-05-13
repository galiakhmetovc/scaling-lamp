import { Box, Button, Divider, Paper, Stack, Typography } from "@mui/material";
import { JsonBlock, KeyValueTable, StatusChip } from "../../components/common";
import type { DebugEntry, ToolCallSummary } from "../../types";
import { formatTime, short } from "../../utils/format";
import { parseToolDebugDetail } from "./toolDetail";

function mapRows(values: Map<string, string>): Array<[string, string]> {
  return Array.from(values.entries()).filter(([, value]) => value.length > 0);
}

function maybeJsonBlock(value: string) {
  try {
    return <JsonBlock value={JSON.parse(value)} />;
  } catch {
    return <Box component="pre" className="chat-tool-detail">{value}</Box>;
  }
}

function parseJson(value?: string | null): unknown | null {
  if (!value) {
    return null;
  }
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringValue(value: unknown): string | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

function extractProcessId(...values: Array<string | null | undefined>): string | null {
  for (const value of values) {
    const match = value?.match(/\bexec-\d+\b/);
    if (match) {
      return match[0];
    }
  }
  return null;
}

function renderStructuredOutput(value: string | null, title: string) {
  const parsed = parseJson(value);
  if (!isRecord(parsed)) {
    return value ? (
      <Stack spacing={0.75}>
        <Typography fontWeight={700}>{title}</Typography>
        <Box component="pre" className="chat-tool-detail">{value}</Box>
      </Stack>
    ) : null;
  }

  const stdout = stringValue(parsed.stdout) ?? stringValue(parsed.output) ?? stringValue(parsed.body);
  const stderr = stringValue(parsed.stderr);
  const exitCode = parsed.exit_code ?? parsed.status_code ?? parsed.status;
  const hasFormattedOutput = stdout !== null || stderr !== null || exitCode !== undefined;
  if (!hasFormattedOutput) {
    return null;
  }

  return (
    <Stack spacing={0.75}>
      <Typography fontWeight={700}>{title}</Typography>
      <KeyValueTable rows={[["Exit/status", exitCode === undefined ? "—" : String(exitCode)]]} />
      {stdout ? (
        <Stack spacing={0.5}>
          <Typography variant="caption" color="text.secondary">
            stdout/output
          </Typography>
          <Box component="pre" className="chat-tool-detail">{stdout}</Box>
        </Stack>
      ) : null}
      {stderr ? (
        <Stack spacing={0.5}>
          <Typography variant="caption" color="error">
            stderr
          </Typography>
          <Box component="pre" className="chat-tool-detail">{stderr}</Box>
        </Stack>
      ) : null}
    </Stack>
  );
}

export function ToolDetailPanel({
  tool,
  toolDetails,
  allTools = [],
  debugEntries = [],
  onClearTool
}: {
  tool: ToolCallSummary;
  toolDetails: DebugEntry | null;
  allTools?: ToolCallSummary[];
  debugEntries?: DebugEntry[];
  onClearTool: () => void;
}) {
  const parsed = parseToolDebugDetail(toolDetails?.detail);
  const resultRows = mapRows(parsed.result);
  const processId = extractProcessId(tool.summary, parsed.arguments, parsed.resultPreview);
  const relatedExecTools = processId
    ? allTools.filter((candidate) => {
        if (candidate.id === tool.id || !candidate.tool_name.startsWith("exec_")) {
          return false;
        }
        const detail = debugEntries.find((entry) => entry.kind === "tool_call" && entry.id === candidate.id);
        return candidate.summary.includes(processId) || (detail?.detail ?? "").includes(processId);
      })
    : [];

  return (
    <Paper variant="outlined" sx={{ p: 1.5, bgcolor: "transparent" }}>
      <Stack direction="row" spacing={1} alignItems="center" justifyContent="space-between" sx={{ mb: 1 }}>
        <Typography fontWeight={700}>Детали tool call</Typography>
        <Stack direction="row" spacing={1} alignItems="center">
          <StatusChip value={tool.status} />
          <Button variant="text" onClick={onClearTool}>
            Закрыть
          </Button>
        </Stack>
      </Stack>
      <KeyValueTable
        rows={[
          ["Tool", tool.tool_name],
          ["Run", short(tool.run_id, 28)],
          ["Call ID", short(tool.id, 28)],
          ["Запрошен", formatTime(tool.requested_at)],
          ["Обновлён", formatTime(tool.updated_at)],
          ["Summary", tool.summary || "—"],
          ["Result", tool.result_summary || "—"],
          ["Artifact", tool.result_artifact_id || "—"]
        ]}
      />
      {tool.error ? (
        <Typography variant="body2" color="error" sx={{ mt: 1 }}>
          {tool.error}
        </Typography>
      ) : null}

      {parsed.arguments ? (
        <Stack spacing={0.75} sx={{ mt: 1.5 }}>
          <Typography fontWeight={700}>Arguments</Typography>
          {maybeJsonBlock(parsed.arguments)}
        </Stack>
      ) : null}

      {resultRows.length > 0 ? (
        <Stack spacing={0.75} sx={{ mt: 1.5 }}>
          <Typography fontWeight={700}>Result metadata</Typography>
          <KeyValueTable rows={resultRows} />
        </Stack>
      ) : null}

      {parsed.resultPreview ? (
        <Stack spacing={0.75} sx={{ mt: 1.5 }}>
          <Typography fontWeight={700}>Result preview</Typography>
          {maybeJsonBlock(parsed.resultPreview)}
        </Stack>
      ) : null}

      {renderStructuredOutput(parsed.resultPreview, "Форматированный вывод")}

      {relatedExecTools.length > 0 ? (
        <Stack spacing={1.25} sx={{ mt: 1.5 }}>
          <Divider />
          <Typography fontWeight={700}>Связанный exec output: {processId}</Typography>
          {relatedExecTools.map((relatedTool) => {
            const relatedDetail = debugEntries.find((entry) => entry.kind === "tool_call" && entry.id === relatedTool.id);
            const relatedParsed = parseToolDebugDetail(relatedDetail?.detail);
            return (
              <Paper key={relatedTool.id} variant="outlined" sx={{ p: 1.25 }}>
                <Stack spacing={1}>
                  <Stack direction="row" justifyContent="space-between" spacing={1}>
                    <Typography className="mono" fontWeight={700}>
                      {relatedTool.tool_name}
                    </Typography>
                    <StatusChip value={relatedTool.status} />
                  </Stack>
                  <Typography variant="caption" color={relatedTool.error ? "error" : "text.secondary"}>
                    {relatedTool.error || relatedTool.summary}
                  </Typography>
                  {renderStructuredOutput(relatedParsed.resultPreview, "Output")}
                </Stack>
              </Paper>
            );
          })}
        </Stack>
      ) : null}

      {!toolDetails?.detail ? (
        <Box component="pre" className="chat-tool-detail">
          {tool.summary}
        </Box>
      ) : null}
    </Paper>
  );
}
