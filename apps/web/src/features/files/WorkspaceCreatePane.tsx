import { Button, Paper, Stack, TextField } from "@mui/material";

export function WorkspaceCreatePane({
  filePath,
  fileContent,
  dirPath,
  saving,
  onFilePathChange,
  onFileContentChange,
  onDirPathChange,
  onCreateFile,
  onCreateDirectory
}: {
  filePath: string;
  fileContent: string;
  dirPath: string;
  saving: boolean;
  onFilePathChange: (value: string) => void;
  onFileContentChange: (value: string) => void;
  onDirPathChange: (value: string) => void;
  onCreateFile: () => void;
  onCreateDirectory: () => void;
}) {
  return (
    <Paper variant="outlined" sx={{ p: 1.5 }}>
      <Stack spacing={1.5}>
        <Stack direction={{ xs: "column", md: "row" }} spacing={1} alignItems="stretch">
          <TextField
            fullWidth
            size="small"
            label="Новый файл"
            value={filePath}
            onChange={(event) => onFilePathChange(event.target.value)}
            placeholder="notes/todo.md"
          />
          <Button variant="contained" disabled={saving || !filePath.trim()} onClick={onCreateFile}>
            Создать файл
          </Button>
        </Stack>
        <TextField
          fullWidth
          multiline
          minRows={4}
          label="Содержимое нового файла"
          value={fileContent}
          onChange={(event) => onFileContentChange(event.target.value)}
          inputProps={{ className: "mono" }}
        />
        <Stack direction={{ xs: "column", md: "row" }} spacing={1} alignItems="stretch">
          <TextField
            fullWidth
            size="small"
            label="Новая папка"
            value={dirPath}
            onChange={(event) => onDirPathChange(event.target.value)}
            placeholder="notes"
          />
          <Button variant="outlined" disabled={saving || !dirPath.trim()} onClick={onCreateDirectory}>
            Создать папку
          </Button>
        </Stack>
      </Stack>
    </Paper>
  );
}
