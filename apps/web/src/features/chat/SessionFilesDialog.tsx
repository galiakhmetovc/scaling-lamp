import { Dialog, DialogContent, DialogTitle, IconButton, Stack, Tab, Tabs, Typography } from "@mui/material";
import { useState } from "react";
import type { SessionSummary } from "../../types";
import { AgentHomeFilesPane } from "../files/AgentHomeFilesPane";
import { ArtifactFilesPane } from "../files/ArtifactFilesPane";
import { WorkspaceFilesPane } from "../files/WorkspaceFilesPane";

type FilesTab = "workspace" | "artifacts" | "agent-home";

export function SessionFilesDialog({
  open,
  selectedSession,
  onClose
}: {
  open: boolean;
  selectedSession: SessionSummary | null;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<FilesTab>("workspace");

  return (
    <Dialog open={open} onClose={onClose} fullWidth maxWidth="xl">
      <DialogTitle>
        <Stack direction="row" spacing={2} alignItems="center" justifyContent="space-between">
          <Stack minWidth={0}>
            <Typography variant="h6">Файлы сессии</Typography>
            <Typography variant="caption" color="text.secondary" className="mono">
              {selectedSession ? `${selectedSession.title || selectedSession.id} · ${selectedSession.id}` : "сессия не выбрана"}
            </Typography>
          </Stack>
          <IconButton aria-label="Закрыть" onClick={onClose}>
            ×
          </IconButton>
        </Stack>
      </DialogTitle>
      <DialogContent dividers className="chat-files-dialog">
        {selectedSession ? (
          <Stack spacing={1.5}>
            <Tabs value={tab} onChange={(_, value: FilesTab) => setTab(value)} variant="scrollable" scrollButtons="auto">
              <Tab value="workspace" label="Workspace" />
              <Tab value="artifacts" label="Artifacts" />
              <Tab value="agent-home" label="Agent home" />
            </Tabs>
            {tab === "workspace" ? <WorkspaceFilesPane sessionId={selectedSession.id} compact /> : null}
            {tab === "artifacts" ? <ArtifactFilesPane sessionId={selectedSession.id} compact /> : null}
            {tab === "agent-home" ? <AgentHomeFilesPane agentId={selectedSession.agent_profile_id} /> : null}
          </Stack>
        ) : (
          <Typography color="text.secondary">Выбери сессию, чтобы открыть её файлы.</Typography>
        )}
      </DialogContent>
    </Dialog>
  );
}
