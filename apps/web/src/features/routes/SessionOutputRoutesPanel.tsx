import { useMemo, useState } from "react";
import {
  Box,
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Divider,
  FormControlLabel,
  MenuItem,
  Paper,
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
import { EmptyState } from "../../components/common";
import type {
  DeliveryTarget,
  SessionOutputRoute,
  SessionOutputRouteCreateOptions,
  SessionOutputRouteUpdatePatch,
  SessionSummary
} from "../../types";
import { sessionTitle } from "../../ui/entityLabels";
import { formatTime, short } from "../../utils/format";
import {
  blankSessionOutputRouteDraft,
  sessionOutputRouteCreateOptionsFromDraft,
  sessionOutputRouteDraftFromRoute,
  sessionOutputRoutePatchFromDraft,
  type SessionOutputRouteDraft
} from "./routeForms";

export function SessionOutputRoutesPanel({
  routes,
  targets,
  sessions,
  onOpenSession,
  onCreate,
  onUpdate
}: {
  routes: SessionOutputRoute[];
  targets: DeliveryTarget[];
  sessions: SessionSummary[];
  onOpenSession: (sessionId: string) => void;
  onCreate: (sessionId: string, targetId: string, options: SessionOutputRouteCreateOptions) => Promise<void>;
  onUpdate: (routeId: string, patch: SessionOutputRouteUpdatePatch) => Promise<void>;
}) {
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<SessionOutputRoute | null>(null);
  const [draft, setDraft] = useState<SessionOutputRouteDraft>(blankSessionOutputRouteDraft());
  const [error, setError] = useState<string | null>(null);
  const latestSessionId = sessions[0]?.id ?? "";
  const firstTargetId = targets[0]?.target_id ?? "";
  const targetById = useMemo(() => new Map(targets.map((target) => [target.target_id, target])), [targets]);

  function startCreate() {
    setDraft(blankSessionOutputRouteDraft(latestSessionId, firstTargetId));
    setError(null);
    setCreating(true);
  }

  function startEdit(route: SessionOutputRoute) {
    setDraft(sessionOutputRouteDraftFromRoute(route));
    setError(null);
    setEditing(route);
  }

  async function save() {
    try {
      setError(null);
      if (editing) {
        await onUpdate(editing.route_id, sessionOutputRoutePatchFromDraft(draft));
        setEditing(null);
        return;
      }
      if (!draft.session_id || !draft.target_id) {
        throw new Error("Нужны session_id и target_id");
      }
      await onCreate(draft.session_id, draft.target_id, sessionOutputRouteCreateOptionsFromDraft(draft));
      setCreating(false);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    }
  }

  const dialogOpen = creating || Boolean(editing);

  return (
    <Paper variant="outlined">
      <Box sx={{ px: 1.5, py: 1 }}>
        <Stack direction="row" justifyContent="space-between" alignItems="center">
          <Typography fontWeight={700}>Session output routes</Typography>
          <Button size="small" variant="contained" onClick={startCreate} disabled={sessions.length === 0 || targets.length === 0}>
            Добавить route
          </Button>
        </Stack>
      </Box>
      <Divider />
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Route</TableCell>
            <TableCell>Сессия</TableCell>
            <TableCell>Target</TableCell>
            <TableCell>Format</TableCell>
            <TableCell>Enabled</TableCell>
            <TableCell>Обновлён</TableCell>
            <TableCell align="right">Действия</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {routes.map((route) => {
            const session = sessionTitle(route.session_id, sessions);
            const target = targetById.get(route.target_id);
            return (
              <TableRow key={route.route_id} hover>
                <TableCell>
                  <Typography fontWeight={700}>{short(route.route_id, 44)}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {route.filter_json}
                  </Typography>
                </TableCell>
                <TableCell>
                  <Button size="small" variant="text" onClick={() => onOpenSession(route.session_id)} sx={{ textTransform: "none" }}>
                    {session.primary}
                  </Button>
                  <Typography variant="caption" color="text.secondary" className="mono" display="block">
                    {short(route.session_id, 28)}
                  </Typography>
                </TableCell>
                <TableCell>
                  <Typography>{target ? `${target.kind} ${target.address}` : route.target_id}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {route.target_id}
                  </Typography>
                </TableCell>
                <TableCell>{route.format_policy}</TableCell>
                <TableCell>{route.enabled ? "да" : "нет"}</TableCell>
                <TableCell>{formatTime(route.updated_at)}</TableCell>
                <TableCell align="right">
                  <Button size="small" variant="outlined" onClick={() => startEdit(route)}>
                    Редактировать
                  </Button>
                </TableCell>
              </TableRow>
            );
          })}
          {routes.length === 0 ? (
            <TableRow>
              <TableCell colSpan={7}>
                <EmptyState title="Session output routes не настроены" />
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>

      <Dialog open={dialogOpen} onClose={() => (editing ? setEditing(null) : setCreating(false))} fullWidth maxWidth="md">
        <DialogTitle>{editing ? "Редактировать session output route" : "Новый session output route"}</DialogTitle>
        <DialogContent>
          <Stack spacing={1.5} sx={{ pt: 1 }}>
            {error ? <Typography color="error">{error}</Typography> : null}
            <TextField
              label="route_id"
              value={draft.route_id}
              disabled={Boolean(editing)}
              onChange={(event) => setDraft({ ...draft, route_id: event.target.value })}
              helperText="Можно оставить пустым, тогда runtime создаст canonical id."
            />
            <TextField
              select
              label="session_id"
              value={draft.session_id}
              disabled={Boolean(editing)}
              onChange={(event) => setDraft({ ...draft, session_id: event.target.value })}
            >
              {sessions.map((session) => (
                <MenuItem key={session.id} value={session.id}>
                  {session.title || session.id} · {session.agent_name}
                </MenuItem>
              ))}
            </TextField>
            <TextField
              select
              label="target_id"
              value={draft.target_id}
              disabled={Boolean(editing)}
              onChange={(event) => setDraft({ ...draft, target_id: event.target.value })}
            >
              {targets.map((target) => (
                <MenuItem key={target.target_id} value={target.target_id}>
                  {target.kind} {target.address} · {target.target_id}
                </MenuItem>
              ))}
            </TextField>
            <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
              <TextField label="format_policy" value={draft.format_policy} onChange={(event) => setDraft({ ...draft, format_policy: event.target.value })} fullWidth />
              <FormControlLabel
                control={<Switch checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.target.checked })} />}
                label="Enabled"
              />
            </Stack>
            <TextField
              label="filter_json"
              value={draft.filter_json}
              onChange={(event) => setDraft({ ...draft, filter_json: event.target.value })}
              multiline
              minRows={4}
              className="mono"
            />
          </Stack>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => (editing ? setEditing(null) : setCreating(false))}>Отмена</Button>
          <Button variant="contained" onClick={() => void save()}>
            Сохранить
          </Button>
        </DialogActions>
      </Dialog>
    </Paper>
  );
}
