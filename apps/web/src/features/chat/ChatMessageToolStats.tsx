import { Chip, Stack } from "@mui/material";
import type { ToolStats } from "./toolStats";

export function ChatMessageToolStats({ stats }: { stats: ToolStats }) {
  if (stats.total === 0) {
    return null;
  }

  return (
    <Stack direction="row" spacing={0.75} flexWrap="wrap" useFlexGap className="chat-message-tool-stats">
      <Chip label={`tools: ${stats.succeeded}/${stats.total}`} color={stats.failed > 0 ? "warning" : "success"} variant="outlined" />
      <Chip label={`errors: ${stats.failed}`} color={stats.failed > 0 ? "error" : "default"} variant="outlined" />
      {stats.mcpTotal > 0 ? (
        <Chip label={`MCP: ${stats.mcpSucceeded}/${stats.mcpTotal}`} color={stats.mcpFailed > 0 ? "warning" : "default"} variant="outlined" />
      ) : null}
    </Stack>
  );
}
