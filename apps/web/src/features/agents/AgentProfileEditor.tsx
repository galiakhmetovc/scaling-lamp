import { Alert, Button, Chip, Paper, Stack, TextField, Typography } from "@mui/material";
import { useEffect, useState } from "react";
import { KeyValueTable } from "../../components/common";
import type { AgentDetail, AgentUpdatePatch, ToolCatalogItem } from "../../types";
import { formatTime } from "../../utils/format";
import { AgentAllowedToolsEditor } from "./AgentAllowedToolsEditor";
import { describeAgentProfileChanges, formatAllowedTools, parseAllowedToolsText } from "./agentProfile";

export function AgentProfileEditor({
  agent,
  toolCatalog,
  saving,
  error,
  notice,
  onSave,
  onDelete
}: {
  agent: AgentDetail;
  toolCatalog: ToolCatalogItem[];
  saving: boolean;
  error: string | null;
  notice: string | null;
  onSave: (patch: AgentUpdatePatch) => void;
  onDelete: () => void;
}) {
  const [name, setName] = useState(agent.name);
  const [workspaceRoot, setWorkspaceRoot] = useState(agent.default_workspace_root ?? "");
  const [allowedToolsText, setAllowedToolsText] = useState(formatAllowedTools(agent.allowed_tools));

  useEffect(() => {
    setName(agent.name);
    setWorkspaceRoot(agent.default_workspace_root ?? "");
    setAllowedToolsText(formatAllowedTools(agent.allowed_tools));
  }, [agent]);

  const parsedTools = parseAllowedToolsText(allowedToolsText);
  const dirty =
    name.trim() !== agent.name ||
    workspaceRoot.trim() !== (agent.default_workspace_root ?? "") ||
    formatAllowedTools(parsedTools) !== formatAllowedTools(agent.allowed_tools);
  const changes = describeAgentProfileChanges({
    currentName: agent.name,
    nextName: name,
    currentWorkspaceRoot: agent.default_workspace_root,
    nextWorkspaceRoot: workspaceRoot,
    currentAllowedTools: agent.allowed_tools,
    nextAllowedTools: parsedTools
  });

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction={{ xs: "column", lg: "row" }} spacing={1.5} justifyContent="space-between">
          <Stack minWidth={0}>
            <Typography variant="h6">{agent.name}</Typography>
            <Typography variant="caption" color="text.secondary" className="mono" sx={{ wordBreak: "break-all" }}>
              {agent.id} · home={agent.agent_home}
            </Typography>
          </Stack>
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Chip label={agent.template_kind} variant="outlined" />
            <Chip label={`tools: ${agent.allowed_tools.length}`} variant="outlined" />
            {dirty ? <Chip label="unsaved" color="warning" /> : null}
          </Stack>
        </Stack>
      </Paper>

      {error ? <Alert severity="error">{error}</Alert> : null}
      {notice ? <Alert severity="success">{notice}</Alert> : null}

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack spacing={1.25}>
          <Typography fontWeight={700}>Метаданные профиля</Typography>
          <KeyValueTable
            rows={[
              ["Created", formatTime(agent.created_at)],
              ["Updated", formatTime(agent.updated_at)],
              ["Template source", agent.created_from_template_id || "—"],
              ["Created by session", agent.created_by_session_id || "—"],
              ["Created by agent", agent.created_by_agent_profile_id || "—"]
            ]}
          />
          <TextField label="Имя агента" value={name} onChange={(event) => setName(event.target.value)} />
          <TextField
            label="Default workspace root"
            value={workspaceRoot}
            onChange={(event) => setWorkspaceRoot(event.target.value)}
            helperText="Пустое значение отключит default workspace root. Путь валидируется backend."
            inputProps={{ className: "mono" }}
          />
          {changes.length > 0 ? (
            <Alert severity="warning">
              <Typography fontWeight={700}>Будут сохранены изменения</Typography>
              <ul className="compact-list">
                {changes.map((change) => (
                  <li key={change}>{change}</li>
                ))}
              </ul>
            </Alert>
          ) : null}
          <AgentAllowedToolsEditor
            allowedTools={parsedTools}
            allowedToolsText={allowedToolsText}
            toolCatalog={toolCatalog}
            onChange={setAllowedToolsText}
          />
          <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
            <Button
              variant="contained"
              disabled={saving || !dirty || !name.trim() || parsedTools.length === 0}
              onClick={() =>
                onSave({
                  name: name.trim(),
                  default_workspace_root: workspaceRoot.trim() || null,
                  allowed_tools: parsedTools
                })
              }
            >
              {saving ? "Сохранение..." : "Сохранить профиль"}
            </Button>
            <Button color="error" variant="outlined" disabled={saving} onClick={onDelete}>
              Удалить профиль
            </Button>
          </Stack>
        </Stack>
      </Paper>
    </Stack>
  );
}
