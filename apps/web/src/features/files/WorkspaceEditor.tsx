import { Alert, Button, Chip, Paper, Stack, TextField, Typography } from "@mui/material";
import type { WorkspaceFile } from "../../types";

export function WorkspaceEditor({
  file,
  content,
  dirty,
  saving,
  onContentChange,
  onSave,
  onDownload
}: {
  file: WorkspaceFile | null;
  content: string;
  dirty: boolean;
  saving: boolean;
  onContentChange: (value: string) => void;
  onSave: () => void;
  onDownload: () => void;
}) {
  if (!file) {
    return null;
  }

  if (!file.text) {
    return (
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack spacing={1}>
          <Typography fontWeight={700} className="mono">
            {file.path}
          </Typography>
          <Alert severity="info">Бинарный файл. Редактирование отключено, доступно скачивание.</Alert>
          <Button variant="outlined" onClick={onDownload}>
            Скачать
          </Button>
        </Stack>
      </Paper>
    );
  }

  return (
    <Paper variant="outlined" sx={{ p: 1.5 }}>
      <Stack spacing={1.25}>
        <Stack direction={{ xs: "column", md: "row" }} justifyContent="space-between" spacing={1}>
          <Stack spacing={0.5} minWidth={0}>
            <Typography fontWeight={700} className="mono" sx={{ wordBreak: "break-word" }}>
              {file.path}
            </Typography>
            <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
              <Chip label={`${file.byte_len} bytes`} variant="outlined" />
              {file.content_truncated ? <Chip label="preview truncated" color="warning" variant="outlined" /> : null}
              {dirty ? <Chip label="unsaved" color="warning" /> : null}
            </Stack>
          </Stack>
          <Stack direction="row" spacing={1}>
            <Button variant="outlined" onClick={onDownload}>
              Скачать
            </Button>
            <Button variant="contained" disabled={saving || !dirty || file.content_truncated} onClick={onSave}>
              {saving ? "Сохранение..." : "Сохранить"}
            </Button>
          </Stack>
        </Stack>
        {file.content_truncated ? (
          <Alert severity="warning">
            Файл показан не полностью. Сохранение отключено, чтобы не перезаписать файл обрезанным preview.
          </Alert>
        ) : null}
        <TextField
          fullWidth
          multiline
          minRows={14}
          value={content}
          onChange={(event) => onContentChange(event.target.value)}
          className="mono"
          inputProps={{ className: "mono" }}
        />
      </Stack>
    </Paper>
  );
}
