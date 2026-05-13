import { useEffect, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Chip,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControl,
  FormControlLabel,
  InputLabel,
  MenuItem,
  Paper,
  Select,
  Stack,
  Switch,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
  Typography
} from "@mui/material";
import { api } from "../../api";
import { EmptyState, JsonBlock, SectionHeader } from "../../components/common";
import type {
  AgentSchedule,
  AgentScheduleDeliveryMode,
  AgentScheduleMode,
  AgentScheduleUpdatePatch,
  AgentSummary,
  SessionSummary
} from "../../types";
import { formatTime, short } from "../../utils/format";
import {
  scheduleDeliveryLabel,
  scheduleModeLabel,
  scheduleStatus,
  scheduleStatusLabel,
  secondsToHuman
} from "./scheduleModel";

function scheduleChipColor(schedule: AgentSchedule): "success" | "default" | "error" {
  const status = scheduleStatus(schedule);
  if (status === "enabled") {
    return "success";
  }
  if (status === "error") {
    return "error";
  }
  return "default";
}

type Draft = {
  id: string;
  agent_identifier: string;
  prompt: string;
  mode: AgentScheduleMode;
  delivery_mode: AgentScheduleDeliveryMode;
  target_session_id: string;
  interval_seconds: number;
  enabled: boolean;
};

const initialDraft: Draft = {
  id: "",
  agent_identifier: "default",
  prompt: "",
  mode: "interval",
  delivery_mode: "fresh_session",
  target_session_id: "",
  interval_seconds: 900,
  enabled: true
};

function draftFromSchedule(schedule: AgentSchedule): Draft {
  return {
    id: schedule.id,
    agent_identifier: schedule.agent_profile_id,
    prompt: schedule.prompt,
    mode: schedule.mode,
    delivery_mode: schedule.delivery_mode,
    target_session_id: schedule.target_session_id ?? "",
    interval_seconds: schedule.interval_seconds,
    enabled: schedule.enabled
  };
}

function patchFromDraft(draft: Draft): AgentScheduleUpdatePatch {
  return {
    agent_identifier: draft.agent_identifier || null,
    prompt: draft.prompt.trim(),
    mode: draft.mode,
    delivery_mode: draft.delivery_mode,
    target_session_id: draft.delivery_mode === "existing_session" ? draft.target_session_id : null,
    interval_seconds: Number(draft.interval_seconds),
    enabled: draft.enabled
  };
}

export function SchedulesScreen({
  agents,
  sessions,
  onOpenSession
}: {
  agents: AgentSummary[];
  sessions: SessionSummary[];
  onOpenSession: (sessionId: string) => void;
}) {
  const [schedules, setSchedules] = useState<AgentSchedule[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [draft, setDraft] = useState<Draft>(initialDraft);
  const [selected, setSelected] = useState<AgentSchedule | null>(null);
  const [editing, setEditing] = useState<AgentSchedule | null>(null);
  const [editDraft, setEditDraft] = useState<Draft>(initialDraft);

  async function loadSchedules(signal?: AbortSignal) {
    setLoading(true);
    setError(null);
    try {
      const response = await api.agentSchedules(signal);
      setSchedules(response.schedules);
    } catch (loadError) {
      if (!signal?.aborted) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    } finally {
      if (!signal?.aborted) {
        setLoading(false);
      }
    }
  }

  useEffect(() => {
    const controller = new AbortController();
    void loadSchedules(controller.signal);
    return () => controller.abort();
  }, []);

  async function createSchedule() {
    const id = draft.id.trim();
    const prompt = draft.prompt.trim();
    if (!id || !prompt) {
      setNotice("Укажи id и prompt расписания.");
      return;
    }
    if (draft.delivery_mode === "existing_session" && !draft.target_session_id) {
      setNotice("Для delivery existing_session нужна target session.");
      return;
    }

    try {
      const result = await api.createAgentSchedule(id, {
        agent_identifier: draft.agent_identifier || null,
        prompt,
        mode: draft.mode,
        delivery_mode: draft.delivery_mode,
        target_session_id: draft.delivery_mode === "existing_session" ? draft.target_session_id : null,
        interval_seconds: Number(draft.interval_seconds),
        enabled: draft.enabled
      });
      setNotice(result.message);
      setDraft(initialDraft);
      await loadSchedules();
    } catch (createError) {
      setNotice(createError instanceof Error ? createError.message : String(createError));
    }
  }

  async function setEnabled(schedule: AgentSchedule, enabled: boolean) {
    try {
      const result = await api.updateAgentSchedule(schedule.id, { enabled });
      setNotice(result.message);
      await loadSchedules();
    } catch (updateError) {
      setNotice(updateError instanceof Error ? updateError.message : String(updateError));
    }
  }

  async function deleteSchedule(schedule: AgentSchedule) {
    try {
      const result = await api.deleteAgentSchedule(schedule.id);
      setNotice(result.message);
      if (selected?.id === schedule.id) {
        setSelected(null);
      }
      await loadSchedules();
    } catch (deleteError) {
      setNotice(deleteError instanceof Error ? deleteError.message : String(deleteError));
    }
  }

  async function runNow(schedule: AgentSchedule) {
    try {
      const result = await api.runAgentScheduleNow(schedule.id);
      setNotice(`Расписание ${result.schedule.id} поставлено в ближайший запуск.`);
      await loadSchedules();
    } catch (runError) {
      setNotice(runError instanceof Error ? runError.message : String(runError));
    }
  }

  async function saveEditedSchedule() {
    if (!editing) {
      return;
    }
    const prompt = editDraft.prompt.trim();
    if (!prompt) {
      setNotice("Prompt расписания не может быть пустым.");
      return;
    }
    if (editDraft.delivery_mode === "existing_session" && !editDraft.target_session_id) {
      setNotice("Для delivery existing_session нужна target session.");
      return;
    }
    try {
      const result = await api.updateAgentSchedule(editing.id, patchFromDraft(editDraft));
      setNotice(result.message);
      setEditing(null);
      await loadSchedules();
    } catch (saveError) {
      setNotice(saveError instanceof Error ? saveError.message : String(saveError));
    }
  }

  const errorCount = schedules.filter((schedule) => Boolean(schedule.last_error)).length;
  const enabledCount = schedules.filter((schedule) => schedule.enabled).length;

  return (
    <Stack spacing={2}>
      <SectionHeader
        title="Расписания"
        subtitle="Agent schedules и continue-later: что будет запущено, каким агентом, куда доставится результат и чем завершился последний запуск."
        action={
          <Button variant="outlined" onClick={() => void loadSchedules()} disabled={loading}>
            Обновить
          </Button>
        }
      />

      {notice ? (
        <Alert severity={notice.startsWith("4") || notice.startsWith("5") ? "error" : "info"} onClose={() => setNotice(null)}>
          {notice}
        </Alert>
      ) : null}
      {error ? <Alert severity="error">{error}</Alert> : null}

      <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
        <Chip label={`всего: ${schedules.length}`} variant="outlined" />
        <Chip label={`включено: ${enabledCount}`} color="success" variant="outlined" />
        <Chip label={`ошибки: ${errorCount}`} color={errorCount ? "error" : "default"} variant="outlined" />
      </Stack>

      <Paper variant="outlined" sx={{ p: 2 }}>
        <Typography fontWeight={700} sx={{ mb: 1.5 }}>
          Создать расписание
        </Typography>
        <Stack spacing={1.5}>
          <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
            <TextField
              label="id"
              value={draft.id}
              onChange={(event) => setDraft({ ...draft, id: event.target.value })}
              size="small"
              fullWidth
              placeholder="server-status-15m"
            />
            <FormControl size="small" fullWidth>
              <InputLabel id="schedule-agent-label">Агент</InputLabel>
              <Select
                labelId="schedule-agent-label"
                label="Агент"
                value={draft.agent_identifier}
                onChange={(event) => setDraft({ ...draft, agent_identifier: event.target.value })}
              >
                {agents.map((agent) => (
                  <MenuItem key={agent.id} value={agent.id}>
                    {agent.name} ({agent.id})
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
            <TextField
              label="Интервал, секунд"
              value={draft.interval_seconds}
              onChange={(event) =>
                setDraft({ ...draft, interval_seconds: Number.parseInt(event.target.value || "0", 10) })
              }
              size="small"
              type="number"
              fullWidth
            />
          </Stack>
          <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
            <FormControl size="small" fullWidth>
              <InputLabel id="schedule-mode-label">Mode</InputLabel>
              <Select
                labelId="schedule-mode-label"
                label="Mode"
                value={draft.mode}
                onChange={(event) => setDraft({ ...draft, mode: event.target.value as AgentScheduleMode })}
              >
                <MenuItem value="interval">interval</MenuItem>
                <MenuItem value="after_completion">after_completion</MenuItem>
                <MenuItem value="once">once</MenuItem>
              </Select>
            </FormControl>
            <FormControl size="small" fullWidth>
              <InputLabel id="schedule-delivery-label">Delivery</InputLabel>
              <Select
                labelId="schedule-delivery-label"
                label="Delivery"
                value={draft.delivery_mode}
                onChange={(event) =>
                  setDraft({ ...draft, delivery_mode: event.target.value as AgentScheduleDeliveryMode })
                }
              >
                <MenuItem value="fresh_session">fresh_session</MenuItem>
                <MenuItem value="existing_session">existing_session</MenuItem>
              </Select>
            </FormControl>
            <FormControl size="small" fullWidth disabled={draft.delivery_mode !== "existing_session"}>
              <InputLabel id="schedule-target-label">Target session</InputLabel>
              <Select
                labelId="schedule-target-label"
                label="Target session"
                value={draft.target_session_id}
                onChange={(event) => setDraft({ ...draft, target_session_id: event.target.value })}
              >
                {sessions.map((session) => (
                  <MenuItem key={session.id} value={session.id}>
                    {session.title} ({short(session.id, 18)})
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Stack>
          <TextField
            label="Prompt"
            value={draft.prompt}
            onChange={(event) => setDraft({ ...draft, prompt: event.target.value })}
            size="small"
            multiline
            minRows={3}
            fullWidth
            placeholder="Напиши короткий статус сервера в мониторинговый чат."
          />
          <Stack direction="row" alignItems="center" justifyContent="space-between">
            <FormControlLabel
              control={
                <Switch
                  checked={draft.enabled}
                  onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })}
                />
              }
              label="Включить сразу"
            />
            <Button variant="contained" onClick={() => void createSchedule()}>
              Создать
            </Button>
          </Stack>
        </Stack>
      </Paper>

      {schedules.length === 0 && !loading ? (
        <EmptyState title="Расписаний нет" detail="Создай interval/once schedule или попроси агента использовать continue_later." />
      ) : (
        <Paper variant="outlined" sx={{ overflow: "hidden" }}>
          <Table size="small">
            <TableHead>
              <TableRow>
                <TableCell>ID</TableCell>
                <TableCell>Статус</TableCell>
                <TableCell>Агент</TableCell>
                <TableCell>Режим</TableCell>
                <TableCell>Доставка</TableCell>
                <TableCell>Следующий запуск</TableCell>
                <TableCell>Последний результат</TableCell>
                <TableCell align="right">Действия</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {schedules.map((schedule) => (
                <TableRow key={schedule.id} hover>
                  <TableCell>
                    <Button variant="text" onClick={() => setSelected(schedule)} sx={{ textTransform: "none" }}>
                      {schedule.id}
                    </Button>
                  </TableCell>
                  <TableCell>
                    <Chip
                      label={scheduleStatusLabel(schedule)}
                      color={scheduleChipColor(schedule)}
                      size="small"
                      variant="outlined"
                    />
                  </TableCell>
                  <TableCell>{schedule.agent_profile_id}</TableCell>
                  <TableCell>
                    {scheduleModeLabel(schedule.mode)} · {secondsToHuman(schedule.interval_seconds)}
                  </TableCell>
                  <TableCell>{scheduleDeliveryLabel(schedule.delivery_mode)}</TableCell>
                  <TableCell>{formatTime(schedule.next_fire_at)}</TableCell>
                  <TableCell>
                    {schedule.last_error ? (
                      <Typography color="error" variant="body2">
                        {short(schedule.last_error, 72)}
                      </Typography>
                    ) : (
                      <Typography variant="body2">{short(schedule.last_result, 72)}</Typography>
                    )}
                  </TableCell>
                  <TableCell align="right">
                    <Stack direction="row" spacing={1} justifyContent="flex-end">
                      {schedule.last_session_id ? (
                        <Button size="small" onClick={() => onOpenSession(schedule.last_session_id || "")}>
                          Сессия
                        </Button>
                      ) : null}
                      <Button size="small" onClick={() => void setEnabled(schedule, !schedule.enabled)}>
                        {schedule.enabled ? "Выключить" : "Включить"}
                      </Button>
                      <Button size="small" onClick={() => void runNow(schedule)}>
                        Run now
                      </Button>
                      <Button
                        size="small"
                        variant="outlined"
                        onClick={() => {
                          setEditing(schedule);
                          setEditDraft(draftFromSchedule(schedule));
                        }}
                      >
                        Редактировать
                      </Button>
                      <Button size="small" color="error" onClick={() => void deleteSchedule(schedule)}>
                        Удалить
                      </Button>
                    </Stack>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Paper>
      )}

      <Dialog open={Boolean(selected)} onClose={() => setSelected(null)} fullWidth maxWidth="md">
        <DialogTitle>{selected?.id}</DialogTitle>
        <DialogContent>
          {selected ? (
            <Stack spacing={2}>
              <Box>
                <Typography variant="caption" color="text.secondary">
                  Prompt
                </Typography>
                <Paper variant="outlined" sx={{ p: 1.5, whiteSpace: "pre-wrap" }}>
                  {selected.prompt}
                </Paper>
              </Box>
              <JsonBlock value={selected} />
            </Stack>
          ) : null}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setSelected(null)}>Закрыть</Button>
        </DialogActions>
      </Dialog>
      <Dialog open={Boolean(editing)} onClose={() => setEditing(null)} fullWidth maxWidth="md">
        <DialogTitle>Редактировать расписание: {editing?.id}</DialogTitle>
        <DialogContent>
          <Stack spacing={1.5} sx={{ pt: 1 }}>
            <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
              <TextField label="id" value={editDraft.id} size="small" fullWidth disabled />
              <FormControl size="small" fullWidth>
                <InputLabel id="schedule-edit-agent-label">Агент</InputLabel>
                <Select
                  labelId="schedule-edit-agent-label"
                  label="Агент"
                  value={editDraft.agent_identifier}
                  onChange={(event) => setEditDraft({ ...editDraft, agent_identifier: event.target.value })}
                >
                  {agents.map((agent) => (
                    <MenuItem key={agent.id} value={agent.id}>
                      {agent.name} ({agent.id})
                    </MenuItem>
                  ))}
                </Select>
              </FormControl>
              <TextField
                label="Интервал, секунд"
                value={editDraft.interval_seconds}
                onChange={(event) =>
                  setEditDraft({ ...editDraft, interval_seconds: Number.parseInt(event.target.value || "0", 10) })
                }
                size="small"
                type="number"
                fullWidth
              />
            </Stack>
            <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
              <FormControl size="small" fullWidth>
                <InputLabel id="schedule-edit-mode-label">Mode</InputLabel>
                <Select
                  labelId="schedule-edit-mode-label"
                  label="Mode"
                  value={editDraft.mode}
                  onChange={(event) => setEditDraft({ ...editDraft, mode: event.target.value as AgentScheduleMode })}
                >
                  <MenuItem value="interval">interval</MenuItem>
                  <MenuItem value="after_completion">after_completion</MenuItem>
                  <MenuItem value="once">once</MenuItem>
                </Select>
              </FormControl>
              <FormControl size="small" fullWidth>
                <InputLabel id="schedule-edit-delivery-label">Delivery</InputLabel>
                <Select
                  labelId="schedule-edit-delivery-label"
                  label="Delivery"
                  value={editDraft.delivery_mode}
                  onChange={(event) =>
                    setEditDraft({ ...editDraft, delivery_mode: event.target.value as AgentScheduleDeliveryMode })
                  }
                >
                  <MenuItem value="fresh_session">fresh_session</MenuItem>
                  <MenuItem value="existing_session">existing_session</MenuItem>
                </Select>
              </FormControl>
              <FormControl size="small" fullWidth disabled={editDraft.delivery_mode !== "existing_session"}>
                <InputLabel id="schedule-edit-target-label">Target session</InputLabel>
                <Select
                  labelId="schedule-edit-target-label"
                  label="Target session"
                  value={editDraft.target_session_id}
                  onChange={(event) => setEditDraft({ ...editDraft, target_session_id: event.target.value })}
                >
                  {sessions.map((session) => (
                    <MenuItem key={session.id} value={session.id}>
                      {session.title} ({short(session.id, 18)})
                    </MenuItem>
                  ))}
                </Select>
              </FormControl>
            </Stack>
            <TextField
              label="Prompt"
              value={editDraft.prompt}
              onChange={(event) => setEditDraft({ ...editDraft, prompt: event.target.value })}
              size="small"
              multiline
              minRows={5}
              fullWidth
            />
            <FormControlLabel
              control={
                <Switch
                  checked={editDraft.enabled}
                  onChange={(event) => setEditDraft({ ...editDraft, enabled: event.target.checked })}
                />
              }
              label="Включено"
            />
          </Stack>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setEditing(null)}>Отмена</Button>
          <Button variant="contained" onClick={() => void saveEditedSchedule()}>
            Сохранить
          </Button>
        </DialogActions>
      </Dialog>
    </Stack>
  );
}
