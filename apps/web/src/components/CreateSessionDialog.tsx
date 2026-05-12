import { Button, Dialog, DialogActions, DialogContent, DialogTitle, FormControl, InputLabel, MenuItem, Select, Stack, TextField } from "@mui/material";
import type { SelectChangeEvent } from "@mui/material";
import type { AgentSummary } from "../types";

export function CreateSessionDialog({
  open,
  title,
  agent,
  agents,
  onClose,
  onTitleChange,
  onAgentChange,
  onSubmit
}: {
  open: boolean;
  title: string;
  agent: string;
  agents: AgentSummary[];
  onClose: () => void;
  onTitleChange: (value: string) => void;
  onAgentChange: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <Dialog open={open} onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>Новая сессия</DialogTitle>
      <DialogContent>
        <Stack spacing={2} sx={{ mt: 1 }}>
          <TextField label="Название" value={title} onChange={(event) => onTitleChange(event.target.value)} />
          <FormControl size="small">
            <InputLabel id="session-agent-label">Агент</InputLabel>
            <Select
              labelId="session-agent-label"
              label="Агент"
              value={agent}
              onChange={(event: SelectChangeEvent) => onAgentChange(event.target.value)}
            >
              <MenuItem value="">default runtime</MenuItem>
              {agents.map((agentItem) => (
                <MenuItem key={agentItem.id} value={agentItem.id}>
                  {agentItem.name} ({agentItem.id})
                </MenuItem>
              ))}
            </Select>
          </FormControl>
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
