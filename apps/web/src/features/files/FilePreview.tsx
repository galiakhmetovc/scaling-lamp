import { Alert, Box, Button, Chip, Paper, Stack, Typography } from "@mui/material";
import { JsonBlock } from "../../components/common";
import { MarkdownMessage } from "../../components/MarkdownMessage";

function parseJson(content?: string | null): unknown | null {
  if (!content) {
    return null;
  }
  try {
    return JSON.parse(content);
  } catch {
    return null;
  }
}

function looksLikeMarkdown(title: string, subtitle?: string): boolean {
  return [title, subtitle ?? ""].some((value) => /\.(md|markdown)$/i.test(value));
}

export function FilePreview({
  title,
  subtitle,
  bytes,
  content,
  text,
  truncated,
  onDownload
}: {
  title: string;
  subtitle?: string;
  bytes: number;
  content?: string | null;
  text: boolean;
  truncated: boolean;
  onDownload?: () => void;
}) {
  const parsedJson = parseJson(content);
  const markdown = parsedJson === null && looksLikeMarkdown(title, subtitle);

  return (
    <Paper variant="outlined" sx={{ p: 1.5 }}>
      <Stack spacing={1.25}>
        <Stack direction="row" justifyContent="space-between" alignItems="flex-start" spacing={2}>
          <Box minWidth={0}>
            <Typography fontWeight={700} className="mono" sx={{ wordBreak: "break-word" }}>
              {title}
            </Typography>
            {subtitle ? (
              <Typography variant="caption" color="text.secondary" className="mono">
                {subtitle}
              </Typography>
            ) : null}
          </Box>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap justifyContent="flex-end">
            <Chip label={`${bytes} bytes`} variant="outlined" />
            <Chip label={text ? "text" : "binary"} color={text ? "success" : "warning"} variant="outlined" />
            {truncated ? <Chip label="preview truncated" color="warning" variant="outlined" /> : null}
            {onDownload ? (
              <Button variant="outlined" onClick={onDownload}>
                Скачать
              </Button>
            ) : null}
          </Stack>
        </Stack>
        {text && parsedJson !== null ? (
          <JsonBlock value={parsedJson} />
        ) : text && markdown ? (
          <Box className="file-preview file-preview-markdown">
            <MarkdownMessage content={content ?? ""} />
          </Box>
        ) : text ? (
          <Typography component="pre" className="file-preview">
            {content ?? ""}
          </Typography>
        ) : (
          <Alert severity="info">Бинарный файл. Используй скачивание для полного доступа.</Alert>
        )}
      </Stack>
    </Paper>
  );
}
