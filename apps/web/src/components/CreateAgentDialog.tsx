import { Alert, Button, Dialog, DialogActions, DialogContent, DialogTitle, FormControl, InputLabel, MenuItem, Select, Stack, TextField } from "@mui/material";
import type { SelectChangeEvent } from "@mui/material";

export function CreateAgentDialog({
  open,
  name,
  template,
  onClose,
  onNameChange,
  onTemplateChange,
  onSubmit
}: {
  open: boolean;
  name: string;
  template: string;
  onClose: () => void;
  onNameChange: (value: string) => void;
  onTemplateChange: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog open={open} onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>Создать агента</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ mt: 1 }}>
          <TextField label="Имя агента" value={name} onChange={(event) => onNameChange(event.target.value)} />
          <FormControl size="small">
            <InputLabel id="agent-template-label">Шаблон</InputLabel>
            <Select
              labelId="agent-template-label"
              label="Шаблон"
              value={template}
              onChange={(event: SelectChangeEvent) => onTemplateChange(event.target.value)}
            >
              <MenuItem value="default">default</MenuItem>
              <MenuItem value="judge">judge</MenuItem>
            </Select>
          </FormControl>
          <Alert severity="info">
            Создание идёт через `/v1/agents`. Редактирование файлов профиля и skills будет добавлено отдельными endpoints, чтобы не делать второй runtime.
          </Alert>
        </Stack>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Отмена</Button>
        <Button variant="contained" onClick={onSubmit}>
          Создать
        </Button>
      </DialogActions>
    </Dialog>
  );
}
