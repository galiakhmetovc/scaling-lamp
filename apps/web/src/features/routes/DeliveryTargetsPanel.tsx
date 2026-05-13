import { useState } from "react";
import {
  Box,
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Divider,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
  Typography
} from "@mui/material";
import { EmptyState } from "../../components/common";
import type { AgentSummary, DeliveryTarget, DeliveryTargetCreateOptions, DeliveryTargetUpdatePatch, SessionSummary } from "../../types";
import { describeDeliveryTarget } from "../../ui/entityLabels";
import { formatTime } from "../../utils/format";
import {
  blankDeliveryTargetDraft,
  deliveryTargetCreateOptionsFromDraft,
  deliveryTargetDraftFromTarget,
  deliveryTargetPatchFromDraft,
  type DeliveryTargetDraft
} from "./routeForms";

export function DeliveryTargetsPanel({
  targets,
  sessions,
  agents,
  onCreate,
  onUpdate
}: {
  targets: DeliveryTarget[];
  sessions: SessionSummary[];
  agents: AgentSummary[];
  onCreate: (targetId: string, options: DeliveryTargetCreateOptions) => Promise<void>;
  onUpdate: (targetId: string, patch: DeliveryTargetUpdatePatch) => Promise<void>;
}) {
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<DeliveryTarget | null>(null);
  const [draft, setDraft] = useState<DeliveryTargetDraft>(blankDeliveryTargetDraft());
  const [error, setError] = useState<string | null>(null);

  function startCreate() {
    setDraft(blankDeliveryTargetDraft());
    setError(null);
    setCreating(true);
  }

  function startEdit(target: DeliveryTarget) {
    setDraft(deliveryTargetDraftFromTarget(target));
    setError(null);
    setEditing(target);
  }

  async function save() {
    try {
      setError(null);
      if (editing) {
        await onUpdate(editing.target_id, deliveryTargetPatchFromDraft(draft));
        setEditing(null);
        return;
      }
      const targetId = draft.target_id.trim();
      if (!targetId) {
        throw new Error("target_id обязателен");
      }
      await onCreate(targetId, deliveryTargetCreateOptionsFromDraft(draft));
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
          <Typography fontWeight={700}>Delivery targets</Typography>
          <Button size="small" variant="contained" onClick={startCreate}>
            Добавить target
          </Button>
        </Stack>
      </Box>
      <Divider />
      <Table size="small">
        <TableHead>
          <TableRow>
            <TableCell>Target</TableCell>
            <TableCell>Kind</TableCell>
            <TableCell>Scope</TableCell>
            <TableCell>Format</TableCell>
            <TableCell>Обновлён</TableCell>
            <TableCell align="right">Действия</TableCell>
          </TableRow>
        </TableHead>
        <TableBody>
          {targets.map((target) => {
            const label = describeDeliveryTarget(target, sessions, agents);
            return (
              <TableRow key={target.target_id} hover>
                <TableCell>
                  <Typography fontWeight={700}>{label.primary}</Typography>
                  <Typography variant="caption" color="text.secondary" className="mono">
                    {label.technical}
                  </Typography>
                </TableCell>
                <TableCell>{target.kind}</TableCell>
                <TableCell>{label.secondary}</TableCell>
                <TableCell>{target.format_policy}</TableCell>
                <TableCell>{formatTime(target.updated_at)}</TableCell>
                <TableCell align="right">
                  <Button size="small" variant="outlined" onClick={() => startEdit(target)}>
                    Редактировать
                  </Button>
                </TableCell>
              </TableRow>
            );
          })}
          {targets.length === 0 ? (
            <TableRow>
              <TableCell colSpan={6}>
                <EmptyState title="Delivery targets не настроены" />
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>

      <Dialog open={dialogOpen} onClose={() => (editing ? setEditing(null) : setCreating(false))} fullWidth maxWidth="md">
        <DialogTitle>{editing ? "Редактировать delivery target" : "Новый delivery target"}</DialogTitle>
        <DialogContent>
          <Stack spacing={1.5} sx={{ pt: 1 }}>
            {error ? <Typography color="error">{error}</Typography> : null}
            <TextField
              label="target_id"
              value={draft.target_id}
              disabled={Boolean(editing)}
              onChange={(event) => setDraft({ ...draft, target_id: event.target.value })}
            />
            <Stack direction={{ xs: "column", md: "row" }} spacing={1.5}>
              <TextField label="kind" value={draft.kind} onChange={(event) => setDraft({ ...draft, kind: event.target.value })} fullWidth />
              <TextField label="format_policy" value={draft.format_policy} onChange={(event) => setDraft({ ...draft, format_policy: event.target.value })} fullWidth />
              <TextField label="scope" value={draft.scope} onChange={(event) => setDraft({ ...draft, scope: event.target.value })} fullWidth />
            </Stack>
            <TextField label="address" value={draft.address} onChange={(event) => setDraft({ ...draft, address: event.target.value })} />
            <TextField label="owner_user_id" value={draft.owner_user_id} onChange={(event) => setDraft({ ...draft, owner_user_id: event.target.value })} />
            <TextField
              label="allowed_agent_ids"
              value={draft.allowed_agent_ids}
              onChange={(event) => setDraft({ ...draft, allowed_agent_ids: event.target.value })}
              multiline
              minRows={2}
              helperText="Один id на строку или через запятую. Пусто = без ограничения."
            />
            <TextField
              label="allowed_session_ids"
              value={draft.allowed_session_ids}
              onChange={(event) => setDraft({ ...draft, allowed_session_ids: event.target.value })}
              multiline
              minRows={2}
              helperText="Один id на строку или через запятую. Пусто = без ограничения."
            />
            <TextField
              label="send_policy_json"
              value={draft.send_policy_json}
              onChange={(event) => setDraft({ ...draft, send_policy_json: event.target.value })}
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
