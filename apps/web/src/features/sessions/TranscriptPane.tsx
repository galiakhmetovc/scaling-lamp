import { Chip, Paper, Stack, Typography } from "@mui/material";
import { EmptyState, StatusChip } from "../../components/common";
import { MarkdownMessage } from "../../components/MarkdownMessage";
import type { SessionTranscript } from "../../types";
import { formatTime, short } from "../../utils/format";

export function TranscriptPane({ transcript }: { transcript: SessionTranscript | null }) {
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
          {entry.role === "assistant" ? (
            <MarkdownMessage content={entry.content} />
          ) : (
            <Typography component="pre" className="transcript-text">
              {entry.content}
            </Typography>
          )}
        </Paper>
      ))}
    </Stack>
  );
}
