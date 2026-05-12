import { Alert, Box, Button, Chip, Paper, Stack, Typography } from "@mui/material";

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
  onDownload: () => void;
}) {
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
            <Button variant="outlined" onClick={onDownload}>
              Скачать
            </Button>
          </Stack>
        </Stack>
        {text ? (
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
