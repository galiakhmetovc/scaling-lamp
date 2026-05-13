import { Box, Button, Paper, Stack, Typography } from "@mui/material";
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

export function ToolDetailPanel({
  tool,
  toolDetails,
  onClearTool
}: {
  tool: ToolCallSummary;
  toolDetails: DebugEntry | null;
  onClearTool: () => void;
}) {
  const parsed = parseToolDebugDetail(toolDetails?.detail);
  const resultRows = mapRows(parsed.result);

  return (
    <Paper variant="outlined" sx={{ p: 1.5 }}>
      <Stack direction="row" spacing={1} alignItems="center" justifyContent="space-between" sx={{ mb: 1 }}>
        <Typography fontWeight={700}>Детали tool call</Typography>
        <Stack direction="row" spacing={1} alignItems="center">
          <StatusChip value={tool.status} />
          <Button variant="text" onClick={onClearTool}>
            Общий вид
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

      {!toolDetails?.detail ? (
        <Box component="pre" className="chat-tool-detail">
          {tool.summary}
        </Box>
      ) : null}
    </Paper>
  );
}
